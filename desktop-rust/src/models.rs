use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct User {
    pub id: i64,
    pub public_id: i64,
    pub name: String,
    pub email: String,
    pub username: String,
    pub bio: String,
    pub phone: String,
    pub birthday: String,
    pub avatar_url: String,
    pub avatar_data: String,
    pub online: bool,
    pub is_premium: bool,
    pub is_admin: bool,
}

impl User {
    pub fn display_name(&self) -> String {
        if !self.name.trim().is_empty() { return self.name.trim().to_owned(); }
        if !self.username.trim().is_empty() { return format!("@{}", self.username.trim().trim_start_matches('@')); }
        if self.public_id > 0 { return format!("Onix {}", self.public_id); }
        "Пользователь".into()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Conversation {
    pub id: i64,
    pub public_id: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub description: String,
    pub avatar_url: String,
    pub avatar_data: String,
    pub member_count: i64,
    pub members_count: i64,
    pub peer: Option<User>,
    pub current_user_role: String,
    pub can_admin: bool,
    pub is_public: bool,
}

impl Conversation {
    pub fn display_title(&self) -> String {
        if self.kind.eq_ignore_ascii_case("private") {
            if let Some(peer) = &self.peer { return peer.display_name(); }
        }
        if !self.title.trim().is_empty() { return self.title.trim().to_owned(); }
        match self.kind.as_str() {
            "saved" => "Избранное".into(), "group" => "Группа".into(), "channel" => "Канал".into(), _ => "Диалог".into()
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Reaction { pub reaction: String, pub count: i64, pub mine: bool, pub paid: bool }

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub conversation_id: i64,
    pub sender_id: i64,
    pub sender_name: String,
    pub sender_username: String,
    pub sender_avatar_data: String,
    pub body: String,
    pub kind: String,
    pub metadata: Value,
    pub created_at: String,
    pub deleted: bool,
    pub pinned: bool,
    pub reactions: Vec<Reaction>,
}

impl Message {
    pub fn display_body(&self) -> String {
        if self.deleted { return "Сообщение удалено".into(); }
        if !self.body.trim().is_empty() { return self.body.trim().to_owned(); }
        if self.metadata.get("voice").is_some() || self.kind == "voice" { return "Голосовое сообщение".into(); }
        if self.metadata.get("sticker").is_some() || self.kind == "sticker" { return "Стикер".into(); }
        if self.metadata.get("gif").is_some() || self.kind == "gif" { return "GIF".into(); }
        if self.metadata.get("files").is_some() || self.kind == "file" { return "Файл".into(); }
        "Сообщение".into()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ChatItem {
    pub conversation: Conversation,
    pub messages: Vec<Message>,
    #[serde(rename = "pinnedMessages")]
    pub pinned_messages: Vec<Message>,
}

impl ChatItem {
    pub fn preview(&self) -> String {
        self.messages.last().map(|m| m.display_body()).unwrap_or_else(|| {
            if self.conversation.description.trim().is_empty() { "Сообщений пока нет".into() } else { self.conversation.description.clone() }
        })
    }
    pub fn recipient_id(&self) -> i64 { self.conversation.peer.as_ref().map(|p| p.id).unwrap_or_default() }
}

#[derive(Debug, Default, Deserialize)]
pub struct Envelope { pub ok: bool, pub message: String }
#[derive(Debug, Default, Deserialize)] pub struct LoginResponse { #[serde(flatten)] pub envelope: Envelope, pub user: User }
#[derive(Debug, Default, Deserialize)] pub struct MeResponse { #[serde(flatten)] pub envelope: Envelope, pub user: User }
#[derive(Debug, Default, Deserialize)] pub struct ChatListResponse { #[serde(flatten)] pub envelope: Envelope, pub items: Vec<ChatItem> }
#[derive(Debug, Default, Deserialize)] #[serde(default, rename_all = "camelCase")]
pub struct ConversationResponse { #[serde(flatten)] pub envelope: Envelope, pub conversation: Conversation, pub messages: Vec<Message>, pub pinned_messages: Vec<Message> }
#[derive(Debug, Default, Deserialize)] #[serde(default)] pub struct SendResponse { #[serde(flatten)] pub envelope: Envelope, pub conversation: Conversation, pub messages: Vec<Message> }
#[derive(Debug, Default, Deserialize)] #[serde(default)] pub struct SingleConversationResponse { #[serde(flatten)] pub envelope: Envelope, pub conversation: Conversation }
#[derive(Debug, Default, Deserialize)] #[serde(default)] pub struct UploadFile { pub id: i64, pub name: String, pub mime: String, pub size: i64, pub url: String, pub kind: String }
#[derive(Debug, Default, Deserialize)] #[serde(default)] pub struct UploadResponse { #[serde(flatten)] pub envelope: Envelope, pub file: UploadFile }
