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
use slint::{ModelRc, SharedString, VecModel};
use std::{env, fs, path::{Path, PathBuf}, rc::Rc, sync::{Arc, atomic::{AtomicBool, Ordering}}, thread, time::Duration};

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
        api: ApiClient::new(&detect_server_url(), settings.session.clone())?,
        settings_path,
        settings: Mutex::new(settings), user: Mutex::new(User::default()), chats: Mutex::new(Vec::new()),
        active_index: Mutex::new(None), active_messages: Mutex::new(Vec::new()), stop: AtomicBool::new(false),
    });

    bind_window_controls(&ui);
    bind_callbacks(&ui, state.clone());
    boot(&ui, state.clone());
    start_poller(&ui, state.clone());
    ui.run()?;
    state.stop.store(true, Ordering::Relaxed);
    Ok(())
}

fn detect_server_url() -> String {
    if let Ok(url) = env::var("ONIX_SERVER_URL") {
        let url = url.trim().trim_end_matches('/');
        if url.starts_with("http://") || url.starts_with("https://") {
            return url.to_owned();
        }
    }
    if let Ok(port) = env::var("ONIX_PORT") {
        if let Ok(port) = port.trim().parse::<u16>() {
            return format!("http://localhost:{port}");
        }
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("onix-config.cmd"));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("onix-config.cmd"));
            }
        }
    }
    if let Ok(dir) = env::current_dir() {
        candidates.push(dir.join("onix-config.cmd"));
    }
    for path in candidates {
        if let Some(port) = read_onix_port(&path) {
            return format!("http://localhost:{port}");
        }
    }
    "http://localhost:8080".to_owned()
}

fn read_onix_port(path: &Path) -> Option<u16> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        let value = line
            .strip_prefix("set \"ONIX_PORT=")
            .map(|v| v.trim_end_matches('"'))
            .or_else(|| line.strip_prefix("set ONIX_PORT="))
            .or_else(|| line.strip_prefix("ONIX_PORT="));
        if let Some(value) = value {
            if let Ok(port) = value.trim().parse::<u16>() {
                return Some(port);
            }
        }
    }
    None
}

fn settings_path() -> PathBuf {
    let base = ProjectDirs::from("com", "Onix", "OnixMessenger").map(|p| p.config_dir().to_owned()).unwrap_or_else(|| PathBuf::from("."));
    let _ = fs::create_dir_all(&base); base.join("settings.json")
}
fn load_settings(path: &PathBuf) -> Settings { fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() }
fn save_settings(state: &State) {
    let mut s = state.settings.lock(); s.session = state.api.session();
    if let Ok(data) = serde_json::to_vec_pretty(&*s) { let _ = fs::write(&state.settings_path, data); }
}

fn bind_window_controls(ui: &AppWindow) {
    use slint::winit_030::WinitWindowAccessor;
    let weak = ui.as_weak(); ui.on_close_window(move || { if let Some(ui) = weak.upgrade() { ui.hide().ok(); slint::quit_event_loop().ok(); } });
    let weak = ui.as_weak(); ui.on_minimize_window(move || { if let Some(ui) = weak.upgrade() { ui.window().set_minimized(true); } });
    let weak = ui.as_weak(); ui.on_maximize_window(move || { if let Some(ui) = weak.upgrade() { let max = ui.window().is_maximized(); ui.window().set_maximized(!max); } });
    let weak = ui.as_weak(); ui.on_drag_window(move || { if let Some(ui) = weak.upgrade() { ui.window().with_winit_window(|w| { let _ = w.drag_window(); }); } });
}

fn bind_callbacks(ui: &AppWindow, state: Arc<State>) {
    let weak = ui.as_weak(); let s = state.clone();
    ui.on_login(move |identifier, password| {
        set_status(&weak, "Подключение к серверу…", false);
        let weak = weak.clone(); let s = s.clone();
        thread::spawn(move || match s.api.login(identifier.as_str(), password.as_str()) {
            Ok(user) => { *s.user.lock() = user; save_settings(&s); authenticate_ui(&weak, &s); refresh_chats(&weak, &s); }
            Err(e) => set_status(&weak, &e.to_string(), true),
        });
    });
    let weak = ui.as_weak(); let s = state.clone();
    ui.on_register(move |name, email, username, password, confirm| {
        set_status(&weak, "Создание аккаунта…", false);
        let weak = weak.clone(); let s = s.clone();
        thread::spawn(move || match s.api.register(name.as_str(), email.as_str(), username.as_str(), password.as_str(), confirm.as_str()) {
            Ok(user) => { *s.user.lock() = user; save_settings(&s); authenticate_ui(&weak, &s); refresh_chats(&weak, &s); }
            Err(e) => set_status(&weak, &e.to_string(), true),
        });
    });
    let weak = ui.as_weak(); let s = state.clone();
    ui.on_select_chat(move |index| { if index >= 0 { open_chat(&weak, &s, index as usize); } });
    let weak = ui.as_weak(); let s = state.clone();
    ui.on_send_message(move |body| {
        let text = body.trim().to_owned(); if text.is_empty() { return; }
        let weak = weak.clone(); let s = s.clone();
        thread::spawn(move || {
            let index = *s.active_index.lock(); let Some(index) = index else { return; };
            let chat = s.chats.lock().get(index).cloned(); let Some(chat) = chat else { return; };
            match s.api.send_message(chat.conversation.id, chat.recipient_id(), &text, json!({"kind":"text"})) {
                Ok(_) => open_chat(&weak, &s, index), Err(e) => set_main_status(&weak, &e.to_string(), true)
            }
        });
    });
    let weak = ui.as_weak(); let s = state.clone(); ui.on_refresh(move || refresh_chats(&weak, &s));
    let weak = ui.as_weak(); let s = state.clone(); ui.on_logout(move || {
        let _ = s.api.logout(); *s.settings.lock() = Settings::default(); save_settings(&s);
        invoke(&weak, |ui| { ui.set_authenticated(false); ui.set_login_status("Вы вышли из аккаунта".into()); ui.set_chats(empty_chats()); ui.set_messages(empty_messages()); });
    });
    let weak = ui.as_weak(); let s = state.clone(); ui.on_create_conversation(move |kind, title, description| {
        let weak = weak.clone(); let s = s.clone(); thread::spawn(move || match s.api.create_conversation(kind.as_str(), title.as_str(), description.as_str()) {
            Ok(_) => { refresh_chats(&weak, &s); invoke(&weak, |ui| ui.set_modal_kind("".into())); }
            Err(e) => set_main_status(&weak, &e.to_string(), true)
        });
    });
    let weak = ui.as_weak(); let s = state.clone(); ui.on_attach_file(move || {
        let weak = weak.clone(); let s = s.clone();
        thread::spawn(move || {
            let Some(path) = rfd::FileDialog::new().pick_file() else { return; };
            set_main_status(&weak, "Загрузка файла…", false);
            let index = *s.active_index.lock(); let Some(index) = index else { return; };
            let chat = s.chats.lock().get(index).cloned(); let Some(chat) = chat else { return; };
            match s.api.upload(&path).and_then(|f| s.api.send_message(chat.conversation.id, chat.recipient_id(), "", json!({"kind":"file","files":[{"id":f.id,"name":f.name,"url":f.url,"mime":f.mime,"size":f.size}]})).map(|_| ())) {
                Ok(_) => open_chat(&weak, &s, index), Err(e) => set_main_status(&weak, &e.to_string(), true)
            }
        });
    });
    let s = state.clone(); ui.on_theme_changed(move |theme| { s.settings.lock().theme = theme.to_string(); save_settings(&s); });
    let weak = ui.as_weak(); ui.on_simple_action(move |action| {
        let message = match action.as_str() {
            "qr" => "QR-вход будет подключён к серверному QR API после его добавления.",
            "forgot" => "Восстановление пароля выполняется через администратора Onix.",
            "call" => "Нативный аудиозвонок будет открыт в следующем медиамодуле.",
            "video" => "Нативный видеозвонок будет открыт в следующем медиамодуле.",
            _ => "Раздел открыт в нативном интерфейсе Onix.",
        }; set_main_status(&weak, message, false);
    });
}

fn boot(ui: &AppWindow, state: Arc<State>) {
    let weak = ui.as_weak();
    thread::spawn(move || {
        let mut last_error = String::new();
        let mut connected = false;
        for _ in 0..20 {
            match state.api.health() {
                Ok(()) => { connected = true; break; }
                Err(error) => {
                    last_error = error.to_string();
                    thread::sleep(Duration::from_millis(250));
                }
            }
        }
        if !connected {
            set_status(&weak, &format!("Не удалось подключиться к серверу Onix: {last_error}"), true);
            return;
        }
        if state.api.session().is_empty() {
            set_status(&weak, "Сервер Onix подключён", false);
            return;
        }
        match state.api.me() {
            Ok(user) => { *state.user.lock() = user; authenticate_ui(&weak, &state); refresh_chats(&weak, &state); }
            Err(_) => { state.settings.lock().session.clear(); save_settings(&state); set_status(&weak, "Введите данные аккаунта", false); }
        }
    });
}
fn start_poller(ui: &AppWindow, state: Arc<State>) {
    let weak = ui.as_weak(); thread::spawn(move || while !state.stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_secs(4)); if state.stop.load(Ordering::Relaxed) { break; }
        if !state.api.session().is_empty() { refresh_chats(&weak, &state); if let Some(i) = *state.active_index.lock() { open_chat(&weak, &state, i); } }
    });
}
fn authenticate_ui(weak: &slint::Weak<AppWindow>, state: &State) {
    let user = state.user.lock().clone(); invoke(weak, move |ui| {
        ui.set_authenticated(true); ui.set_current_user_name(user.display_name().into()); ui.set_current_user_email(user.email.into()); ui.set_login_status("".into());
    });
}
fn refresh_chats(weak: &slint::Weak<AppWindow>, state: &State) {
    match state.api.chats() {
        Ok(chats) => {
            let rows = chats.iter().map(|c| chat_row(c)).collect::<Vec<_>>(); *state.chats.lock() = chats;
            invoke(weak, move |ui| ui.set_chats(ModelRc::new(VecModel::from(rows))));
        }
        Err(e) => set_main_status(weak, &e.to_string(), true),
    }
}
fn open_chat(weak: &slint::Weak<AppWindow>, state: &State, index: usize) {
    let chat = state.chats.lock().get(index).cloned(); let Some(chat) = chat else { return; };
    match state.api.conversation(chat.conversation.id, chat.recipient_id()) {
        Ok(response) => apply_conversation(weak, state, index, response), Err(e) => set_main_status(weak, &e.to_string(), true)
    }
}
fn apply_conversation(weak: &slint::Weak<AppWindow>, state: &State, index: usize, response: ConversationResponse) {
    let current_user = state.user.lock().id; let rows = response.messages.iter().map(|m| message_row(m, current_user)).collect::<Vec<_>>();
    *state.active_index.lock() = Some(index); *state.active_messages.lock() = response.messages;
    let title = response.conversation.display_title();
    let subtitle = match response.conversation.kind.as_str() { "group" => format!("{} участников", response.conversation.member_count.max(response.conversation.members_count)), "channel" => "канал".into(), "saved" => "личное облако".into(), _ => response.conversation.peer.as_ref().map(|p| if p.online { "в сети" } else { "не в сети" }.to_owned()).unwrap_or_default() };
    let _ = state.api.mark_read(response.conversation.id);
    invoke(weak, move |ui| { ui.set_active_chat_index(index as i32); ui.set_active_chat_title(title.into()); ui.set_active_chat_subtitle(subtitle.into()); ui.set_messages(ModelRc::new(VecModel::from(rows))); ui.set_main_status("".into()); });
}
fn chat_row(chat: &ChatItem) -> ChatRow {
    let title = chat.conversation.display_title(); let letter = title.chars().next().unwrap_or('O').to_uppercase().to_string();
    let time = chat.messages.last().map(|m| format_time(&m.created_at)).unwrap_or_default();
    ChatRow { title: title.into(), preview: chat.preview().into(), time: time.into(), unread: 0, avatar_text: letter.into(), kind: chat.conversation.kind.clone().into() }
}
fn message_row(message: &Message, current_user: i64) -> MessageRow {
    MessageRow { sender: message.sender_name.clone().into(), body: message.display_body().into(), time: format_time(&message.created_at).into(), mine: message.sender_id == current_user, system: false }
}
fn format_time(raw: &str) -> String { DateTime::parse_from_rfc3339(raw).map(|t| t.with_timezone(&Local).format("%H:%M").to_string()).unwrap_or_default() }
fn empty_chats() -> ModelRc<ChatRow> { ModelRc::new(VecModel::from(Vec::<ChatRow>::new())) }
fn empty_messages() -> ModelRc<MessageRow> { ModelRc::new(VecModel::from(Vec::<MessageRow>::new())) }
fn invoke<F: FnOnce(AppWindow) + Send + 'static>(weak: &slint::Weak<AppWindow>, f: F) { let weak = weak.clone(); let _ = slint::invoke_from_event_loop(move || if let Some(ui) = weak.upgrade() { f(ui); }); }
fn set_status(weak: &slint::Weak<AppWindow>, text: &str, error: bool) { let text: SharedString = text.into(); invoke(weak, move |ui| { ui.set_login_status(text); ui.set_status_error(error); }); }
fn set_main_status(weak: &slint::Weak<AppWindow>, text: &str, error: bool) { let text: SharedString = text.into(); invoke(weak, move |ui| { ui.set_main_status(text); ui.set_status_error(error); }); }
