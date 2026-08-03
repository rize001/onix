use crate::models::*;
use anyhow::{anyhow, Context, Result};
use parking_lot::Mutex;
use reqwest::blocking::{multipart, Client, RequestBuilder, Response};
use reqwest::header::{ACCEPT, CONTENT_TYPE, COOKIE, SET_COOKIE, USER_AGENT};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{fs, path::Path, sync::Arc, time::Duration};

#[derive(Clone)]
pub struct ApiClient {
    base: String,
    client: Client,
    session: Arc<Mutex<String>>,
}

impl ApiClient {
    pub fn new(base: &str, session: String) -> Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(45)).no_proxy().build()?;
        Ok(Self { base: base.trim_end_matches('/').into(), client, session: Arc::new(Mutex::new(session)) })
    }
    pub fn session(&self) -> String { self.session.lock().clone() }
    fn decorate(&self, req: RequestBuilder) -> RequestBuilder {
        let token = self.session.lock().clone();
        let req = req.header(ACCEPT, "application/json").header(USER_AGENT, "OnixMessengerRust/161 Windows");
        if token.is_empty() { req } else { req.header(COOKIE, format!("ONIXSESSID={token}")) }
    }
    fn capture_session(&self, response: &Response) {
        for value in response.headers().get_all(SET_COOKIE).iter() {
            if let Ok(value) = value.to_str() {
                if let Some(rest) = value.strip_prefix("ONIXSESSID=") {
                    if let Some(token) = rest.split(';').next() { *self.session.lock() = token.to_owned(); }
                }
            }
        }
    }
    fn decode<T: DeserializeOwned>(&self, response: Response) -> Result<T> {
        self.capture_session(&response);
        let status = response.status();
        let bytes = response.bytes()?;
        if !status.is_success() {
            let value: Value = serde_json::from_slice(&bytes).unwrap_or_default();
            let message = value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Ошибка сервера Onix")
                .to_owned();
            return Err(anyhow!(message));
        }
        serde_json::from_slice(&bytes).context("не удалось разобрать ответ Onix")
    }
    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let response = self.decorate(self.client.get(format!("{}{}", self.base, path))).send()?;
        self.decode(response)
    }
    fn post<T: DeserializeOwned>(&self, path: &str, payload: Value) -> Result<T> {
        let response = self.decorate(self.client.post(format!("{}{}", self.base, path)))
            .header(CONTENT_TYPE, "application/json; charset=utf-8").json(&payload).send()?;
        self.decode(response)
    }
    pub fn health(&self) -> Result<()> { let _: Envelope = self.get("/api/health")?; Ok(()) }
    pub fn login(&self, identifier: &str, password: &str) -> Result<User> {
        Ok(self.post::<LoginResponse>("/api/v2/login", json!({"identifier": identifier.trim(), "password": password}))?.user)
    }
    pub fn register(&self, name: &str, email: &str, username: &str, password: &str, confirm: &str) -> Result<User> {
        Ok(self.post::<LoginResponse>("/api/v2/register", json!({
            "name": name.trim(), "email": email.trim(), "username": username.trim().trim_start_matches('@'),
            "birthday": "", "password": password, "passwordConfirm": confirm
        }))?.user)
    }
    pub fn me(&self) -> Result<User> { Ok(self.get::<MeResponse>("/api/v2/me")?.user) }
    pub fn logout(&self) -> Result<()> {
        let _: Envelope = self.post("/api/v2/logout", json!({}))?;
        self.session.lock().clear(); Ok(())
    }
    pub fn chats(&self) -> Result<Vec<ChatItem>> { Ok(self.post::<ChatListResponse>("/api/v2/messages/list", json!({"afterId": 0}))?.items) }
    pub fn conversation(&self, conversation_id: i64, recipient_id: i64) -> Result<ConversationResponse> {
        self.post("/api/v2/messages/conversation", json!({"conversationId": conversation_id, "recipientId": recipient_id, "afterId": 0}))
    }
    pub fn send_message(&self, conversation_id: i64, recipient_id: i64, body: &str, metadata: Value) -> Result<SendResponse> {
        self.post("/api/v2/messages/send", json!({"conversationId": conversation_id, "recipientId": recipient_id, "body": body, "metadata": metadata}))
    }
    pub fn mark_read(&self, conversation_id: i64) -> Result<()> {
        let _: Envelope = self.post("/api/v2/messages/mark-read", json!({"conversationId": conversation_id}))?; Ok(())
    }
    pub fn create_conversation(&self, kind: &str, title: &str, description: &str) -> Result<Conversation> {
        Ok(self.post::<SingleConversationResponse>("/api/v2/conversations/create", json!({
            "type": kind, "title": title.trim(), "description": description.trim(), "memberIds": [], "isPublic": false
        }))?.conversation)
    }
    pub fn upload(&self, path: &Path) -> Result<UploadFile> {
        let bytes = fs::read(path)?;
        if bytes.len() > 128 * 1024 * 1024 { return Err(anyhow!("Файл больше 128 МБ")); }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("onix-file").to_owned();
        let part = multipart::Part::bytes(bytes).file_name(name);
        let form = multipart::Form::new().part("file", part);
        let response = self.decorate(self.client.post(format!("{}/api/v2/uploads/file", self.base))).multipart(form).send()?;
        Ok(self.decode::<UploadResponse>(response)?.file)
    }
}
