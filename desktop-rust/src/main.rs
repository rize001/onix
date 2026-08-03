#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod models;

use api::ApiClient;
use chrono::{DateTime, Local};
use directories::ProjectDirs;
use models::{ChatItem, ConversationResponse, Message, User};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{fs, path::PathBuf, sync::{Arc, atomic::{AtomicBool, Ordering}}, thread, time::Duration};

slint::include_modules!();

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings { session: String, theme: String }
impl Default for Settings { fn default() -> Self { Self { session: String::new(), theme: "ember".into() } } }

struct State {
    api: ApiClient,
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    user: Mutex<User>,
    chats: Mutex<Vec<ChatItem>>,
    active_index: Mutex<Option<usize>>,
    active_messages: Mutex<Vec<Message>>,
    stop: AtomicBool,
}

fn main() -> anyhow::Result<()> {
    slint::BackendSelector::new().backend_name("winit".into()).renderer_name("software".into()).select()?;
    let ui = AppWindow::new()?;
    let settings_path = settings_path();
    let settings = load_settings(&settings_path);
    ui.set_theme_name(settings.theme.clone().into());

    let state = Arc::new(State {
        api: ApiClient::new("http://localhost:8080", settings.session.clone())?,
        settings_path,
        settings: Mutex::new(settings),
        user: Mutex::new(User::default()),
        chats: Mutex::new(Vec::new()),
        active_index: Mutex::new(None),
        active_messages: Mutex::new(Vec::new()),
        stop: AtomicBool::new(false),
    });

    bind_window_controls(&ui);
    bind_callbacks(&ui, state.clone());
    boot(&ui, state.clone());
    start_poller(&ui, state.clone());
    ui.run()?;
    state.stop.store(true, Ordering::Relaxed);
    Ok(())
}

fn settings_path() -> PathBuf {
    let base = ProjectDirs::from("com", "Onix", "OnixMessenger")
        .map(|p| p.config_dir().to_owned()).unwrap_or_else(|| PathBuf::from("."));
    let _ = fs::create_dir_all(&base);
    base.join("settings.json")
}
fn load_settings(path: &PathBuf) -> Settings {
    fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}
fn save_settings(state: &State) {
    let mut settings = state.settings.lock();
    settings.session = state.api.session();
    if let Ok(data) = serde_json::to_vec_pretty(&*settings) { let _ = fs::write(&state.settings_path, data); }
}

fn bind_window_controls(ui: &AppWindow) {
    use slint::winit_030::WinitWindowAccessor;
    let weak = ui.as_weak();
    ui.on_close_window(move || { if let Some(ui) = weak.upgrade() { let _ = ui.hide(); let _ = slint::quit_event_loop(); } });
    let weak = ui.as_weak();
    ui.on_minimize_window(move || { if let Some(ui) = weak.upgrade() { ui.window().set_minimized(true); } });
    let weak = ui.as_weak();
    ui.on_maximize_window(move || { if let Some(ui) = weak.upgrade() { let value = !ui.window().is_maximized(); ui.window().set_maximized(value); } });
    let weak = ui.as_weak();
    ui.on_drag_window(move || { if let Some(ui) = weak.upgrade() { ui.window().with_winit_window(|window| { let _ = window.drag_window(); }); } });
}

fn bind_callbacks(ui: &AppWindow, state: Arc<State>) {
    let weak = ui.as_weak(); let state_login = state.clone();
    ui.on_login(move |identifier, password| {
        set_login_status(&weak, "Подключение к серверу…", false);
        let weak = weak.clone(); let state = state_login.clone();
        thread::spawn(move || match state.api.login(identifier.as_str(), password.as_str()) {
            Ok(user) => { *state.user.lock() = user; save_settings(&state); authenticate_ui(&weak, &state); refresh_chats(&weak, &state); }
            Err(error) => set_login_status(&weak, &error.to_string(), true),
        });
    });

    let weak = ui.as_weak(); let state_register = state.clone();
    ui.on_register(move |name, email, username, password, confirm| {
        set_login_status(&weak, "Создание аккаунта…", false);
        let weak = weak.clone(); let state = state_register.clone();
        thread::spawn(move || match state.api.register(name.as_str(), email.as_str(), username.as_str(), password.as_str(), confirm.as_str()) {
            Ok(user) => { *state.user.lock() = user; save_settings(&state); authenticate_ui(&weak, &state); refresh_chats(&weak, &state); }
            Err(error) => set_login_status(&weak, &error.to_string(), true),
        });
    });

    let weak = ui.as_weak(); let state_select = state.clone();
    ui.on_select_chat(move |index| { if index >= 0 { open_chat(&weak, &state_select, index as usize); } });

    let weak = ui.as_weak(); let state_send = state.clone();
    ui.on_send_message(move |body| {
        let body = body.trim().to_owned();
        if body.is_empty() { return; }
        let weak = weak.clone(); let state = state_send.clone();
        thread::spawn(move || {
            let Some(index) = *state.active_index.lock() else { return; };
            let Some(chat) = state.chats.lock().get(index).cloned() else { return; };
            match state.api.send_message(chat.conversation.id, chat.recipient_id(), &body, json!({"kind":"text"})) {
                Ok(_) => open_chat(&weak, &state, index),
                Err(error) => set_main_status(&weak, &error.to_string(), true),
            }
        });
    });

    let weak = ui.as_weak(); let state_refresh = state.clone();
    ui.on_refresh(move || refresh_chats(&weak, &state_refresh));

    let weak = ui.as_weak(); let state_logout = state.clone();
    ui.on_logout(move || {
        let _ = state_logout.api.logout();
        *state_logout.settings.lock() = Settings::default();
        save_settings(&state_logout);
        invoke(&weak, |ui| {
            ui.set_authenticated(false);
            ui.set_login_status("Вы вышли из аккаунта".into());
            ui.set_chats(empty_chats());
            ui.set_messages(empty_messages());
        });
    });

    let weak = ui.as_weak(); let state_create = state.clone();
    ui.on_create_conversation(move |kind, title, description| {
        let weak = weak.clone(); let state = state_create.clone();
        thread::spawn(move || match state.api.create_conversation(kind.as_str(), title.as_str(), description.as_str()) {
            Ok(_) => { refresh_chats(&weak, &state); invoke(&weak, |ui| ui.set_modal_kind("".into())); }
            Err(error) => set_main_status(&weak, &error.to_string(), true),
        });
    });

    let weak = ui.as_weak(); let state_attach = state.clone();
    ui.on_attach_file(move || {
        let weak = weak.clone(); let state = state_attach.clone();
        thread::spawn(move || {
            let Some(path) = rfd::FileDialog::new().pick_file() else { return; };
            set_main_status(&weak, "Загрузка файла…", false);
            let Some(index) = *state.active_index.lock() else { return; };
            let Some(chat) = state.chats.lock().get(index).cloned() else { return; };
            let result = state.api.upload(&path).and_then(|file| {
                state.api.send_message(chat.conversation.id, chat.recipient_id(), "", json!({
                    "kind":"file", "files":[{"id":file.id,"name":file.name,"url":file.url,"mime":file.mime,"size":file.size}]
                })).map(|_| ())
            });
            match result { Ok(_) => open_chat(&weak, &state, index), Err(error) => set_main_status(&weak, &error.to_string(), true) }
        });
    });

    let state_theme = state.clone();
    ui.on_theme_changed(move |theme| { state_theme.settings.lock().theme = theme.to_string(); save_settings(&state_theme); });

    let weak = ui.as_weak();
    ui.on_simple_action(move |action| {
        let message = match action.as_str() {
            "qr" => "QR-вход будет подключён к серверному QR API после его добавления.",
            "forgot" => "Восстановление пароля выполняется через администратора Onix.",
            "call" => "Открытие нативного аудиозвонка…",
            "video" => "Открытие нативного видеозвонка…",
            _ => "Раздел открыт в нативном интерфейсе Onix.",
        };
        set_main_status(&weak, message, false);
    });
}

fn boot(ui: &AppWindow, state: Arc<State>) {
    let weak = ui.as_weak();
    thread::spawn(move || {
        if let Err(error) = state.api.health() {
            set_login_status(&weak, &format!("Запусти server.bat: {error}"), true);
            return;
        }
        if state.api.session().is_empty() {
            set_login_status(&weak, "Сервер Onix подключён", false);
            return;
        }
        match state.api.me() {
            Ok(user) => { *state.user.lock() = user; authenticate_ui(&weak, &state); refresh_chats(&weak, &state); }
            Err(_) => { state.settings.lock().session.clear(); save_settings(&state); set_login_status(&weak, "Введите данные аккаунта", false); }
        }
    });
}

fn start_poller(ui: &AppWindow, state: Arc<State>) {
    let weak = ui.as_weak();
    thread::spawn(move || while !state.stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_secs(4));
        if state.stop.load(Ordering::Relaxed) { break; }
        if !state.api.session().is_empty() {
            refresh_chats(&weak, &state);
            if let Some(index) = *state.active_index.lock() { open_chat(&weak, &state, index); }
        }
    });
}

fn authenticate_ui(weak: &slint::Weak<AppWindow>, state: &State) {
    let user = state.user.lock().clone();
    invoke(weak, move |ui| {
        ui.set_authenticated(true);
        ui.set_current_user_name(user.display_name().into());
        ui.set_current_user_email(user.email.into());
        ui.set_login_status("".into());
    });
}

fn refresh_chats(weak: &slint::Weak<AppWindow>, state: &State) {
    match state.api.chats() {
        Ok(chats) => {
            let rows = chats.iter().map(chat_row).collect::<Vec<_>>();
            *state.chats.lock() = chats;
            invoke(weak, move |ui| ui.set_chats(ModelRc::new(VecModel::from(rows))));
        }
        Err(error) => set_main_status(weak, &error.to_string(), true),
    }
}

fn open_chat(weak: &slint::Weak<AppWindow>, state: &State, index: usize) {
    let Some(chat) = state.chats.lock().get(index).cloned() else { return; };
    match state.api.conversation(chat.conversation.id, chat.recipient_id()) {
        Ok(response) => apply_conversation(weak, state, index, response),
        Err(error) => set_main_status(weak, &error.to_string(), true),
    }
}

fn apply_conversation(weak: &slint::Weak<AppWindow>, state: &State, index: usize, response: ConversationResponse) {
    let current_user = state.user.lock().id;
    let rows = response.messages.iter().map(|message| message_row(message, current_user)).collect::<Vec<_>>();
    *state.active_index.lock() = Some(index);
    *state.active_messages.lock() = response.messages;
    let title = response.conversation.display_title();
    let subtitle = match response.conversation.kind.as_str() {
        "group" => format!("{} участников", response.conversation.member_count.max(response.conversation.members_count)),
        "channel" => "канал".into(),
        "saved" => "личное облако".into(),
        _ => response.conversation.peer.as_ref().map(|peer| if peer.online { "в сети" } else { "не в сети" }.to_owned()).unwrap_or_default(),
    };
    let _ = state.api.mark_read(response.conversation.id);
    invoke(weak, move |ui| {
        ui.set_active_chat_index(index as i32);
        ui.set_active_chat_title(title.into());
        ui.set_active_chat_subtitle(subtitle.into());
        ui.set_messages(ModelRc::new(VecModel::from(rows)));
        ui.set_main_status("".into());
    });
}

fn chat_row(chat: &ChatItem) -> ChatRow {
    let title = chat.conversation.display_title();
    let letter = title.chars().next().unwrap_or('O').to_uppercase().to_string();
    let time = chat.messages.last().map(|message| format_time(&message.created_at)).unwrap_or_default();
    ChatRow { title: title.into(), preview: chat.preview().into(), time: time.into(), unread: 0, avatar_text: letter.into(), kind: chat.conversation.kind.clone().into() }
}
fn message_row(message: &Message, current_user: i64) -> MessageRow {
    MessageRow { sender: message.sender_name.clone().into(), body: message.display_body().into(), time: format_time(&message.created_at).into(), mine: message.sender_id == current_user, system: false }
}
fn format_time(raw: &str) -> String {
    DateTime::parse_from_rfc3339(raw).map(|time| time.with_timezone(&Local).format("%H:%M").to_string()).unwrap_or_default()
}
fn empty_chats() -> ModelRc<ChatRow> { ModelRc::new(VecModel::from(Vec::<ChatRow>::new())) }
fn empty_messages() -> ModelRc<MessageRow> { ModelRc::new(VecModel::from(Vec::<MessageRow>::new())) }
fn invoke<F: FnOnce(AppWindow) + Send + 'static>(weak: &slint::Weak<AppWindow>, callback: F) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || if let Some(ui) = weak.upgrade() { callback(ui); });
}
fn set_login_status(weak: &slint::Weak<AppWindow>, text: &str, error: bool) {
    let text: SharedString = text.into(); invoke(weak, move |ui| { ui.set_login_status(text); ui.set_status_error(error); });
}
fn set_main_status(weak: &slint::Weak<AppWindow>, text: &str, error: bool) {
    let text: SharedString = text.into(); invoke(weak, move |ui| { ui.set_main_status(text); ui.set_status_error(error); });
}
