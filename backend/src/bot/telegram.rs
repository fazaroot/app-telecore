//! Klien Telegram Bot API (reqwest ke https://api.telegram.org).
//!
//! Semua panggilan memakai token bot yang didekripsi sesaat sebelum dipakai.
//! Token TIDAK PERNAH ditulis ke log — URL request yang mengandung token
//! tidak boleh di-log (lihat `middleware::sanitize_log`).

use reqwest::multipart::{Form, Part};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::time::Duration;

use crate::error::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Tipe respons Telegram
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TelegramResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub description: Option<String>,
    pub error_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramUser {
    pub id: i64,
    pub is_bot: bool,
    pub first_name: String,
    pub last_name: Option<String>,
    pub username: Option<String>,
    pub language_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: String,
    pub title: Option<String>,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoSize {
    pub file_id: String,
    #[serde(default)]
    pub width: i64,
    #[serde(default)]
    pub height: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMessage {
    pub message_id: i64,
    pub from: Option<TelegramUser>,
    pub chat: TelegramChat,
    pub date: i64,
    pub text: Option<String>,
    pub caption: Option<String>,
    #[serde(default)]
    pub photo: Option<Vec<PhotoSize>>,
    #[serde(default)]
    pub new_chat_members: Option<Vec<TelegramUser>>,
    #[serde(default)]
    pub left_chat_member: Option<TelegramUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Update {
    pub update_id: i64,
    #[serde(default)]
    pub message: Option<TelegramMessage>,
    #[serde(default)]
    pub edited_message: Option<TelegramMessage>,
    #[serde(default)]
    pub channel_post: Option<TelegramMessage>,
    #[serde(default)]
    pub callback_query: Option<serde_json::Value>,
    #[serde(default)]
    pub my_chat_member: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMember {
    pub user: TelegramUser,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct WebhookInfo {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub has_custom_certificate: bool,
    #[serde(default)]
    pub pending_update_count: i64,
    #[serde(default)]
    pub last_error_date: Option<i64>,
    #[serde(default)]
    pub last_error_message: Option<String>,
    #[serde(default)]
    pub max_connections: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct TgBotCommand {
    pub command: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Parameter kirim pesan
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Default)]
pub struct SendMessageParams {
    pub chat_id: i64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_notification: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SendMediaParams {
    pub chat_id: i64,
    /// file_id Telegram atau URL publik (JSON API). Upload file mentah
    /// memakai `send_media_bytes` (multipart).
    #[serde(skip_serializing)]
    pub media: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct TelegramClient {
    http: reqwest::Client,
    base_url: String,
}

// Semua method client adalah API publik (sebagian dipakai endpoint,
// sebagian disiapkan untuk pemakaian berikutnya) — jangan warn dead_code.
#[allow(dead_code)]
impl TelegramClient {
    pub fn new() -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("TeleCore/0.1 (+https://github.com/fazaroot)")
            .build()?;
        Ok(Self {
            http,
            base_url: "https://api.telegram.org".to_string(),
        })
    }

    fn url(&self, token: &str, method: &str) -> String {
        format!("{}/bot{}/{}", self.base_url, token, method)
    }

    async fn call<T: DeserializeOwned, B: Serialize>(
        &self,
        token: &str,
        method: &str,
        body: &B,
    ) -> AppResult<T> {
        let resp = self
            .http
            .post(self.url(token, method))
            .json(body)
            .send()
            .await
            .map_err(|e| {
                AppError::Telegram(format!("gagal menghubungi api.telegram.org ({method}): {e}"))
            })?;
        self.parse_response(resp, method).await
    }

    async fn call_multipart<T: DeserializeOwned>(
        &self,
        token: &str,
        method: &str,
        form: Form,
    ) -> AppResult<T> {
        let resp = self
            .http
            .post(self.url(token, method))
            .multipart(form)
            .send()
            .await
            .map_err(|e| {
                AppError::Telegram(format!("gagal menghubungi api.telegram.org ({method}): {e}"))
            })?;
        self.parse_response(resp, method).await
    }

    async fn parse_response<T: DeserializeOwned>(
        &self,
        resp: reqwest::Response,
        method: &str,
    ) -> AppResult<T> {
        let parsed: TelegramResponse<T> = resp.json().await.map_err(|e| {
            AppError::Telegram(format!("respons Telegram tidak valid ({method}): {e}"))
        })?;
        if parsed.ok {
            parsed
                .result
                .ok_or_else(|| AppError::Telegram(format!("respons Telegram ok tanpa result ({method})")))
        } else {
            Err(AppError::Telegram(format!(
                "Telegram API error {} ({}): {}",
                parsed.error_code.unwrap_or(0),
                method,
                parsed.description.unwrap_or_default()
            )))
        }
    }

    // --- Info bot ---

    pub async fn get_me(&self, token: &str) -> AppResult<TelegramUser> {
        self.call(token, "getMe", &serde_json::json!({})).await
    }

    // --- Kirim pesan ---

    pub async fn send_message(
        &self,
        token: &str,
        params: &SendMessageParams,
    ) -> AppResult<TelegramMessage> {
        self.call(token, "sendMessage", params).await
    }

    /// Kirim media via JSON (file_id atau URL publik).
    async fn send_media_json(
        &self,
        token: &str,
        method: &str,
        field: &str,
        params: &SendMediaParams,
    ) -> AppResult<TelegramMessage> {
        let mut body = serde_json::json!({
            "chat_id": params.chat_id,
            field: params.media,
        });
        if let Some(c) = &params.caption {
            body["caption"] = serde_json::Value::String(c.clone());
        }
        if let Some(p) = &params.parse_mode {
            body["parse_mode"] = serde_json::Value::String(p.clone());
        }
        if let Some(m) = &params.reply_markup {
            body["reply_markup"] = m.clone();
        }
        self.call(token, method, &body).await
    }

    pub async fn send_photo(&self, token: &str, p: &SendMediaParams) -> AppResult<TelegramMessage> {
        self.send_media_json(token, "sendPhoto", "photo", p).await
    }

    pub async fn send_document(&self, token: &str, p: &SendMediaParams) -> AppResult<TelegramMessage> {
        self.send_media_json(token, "sendDocument", "document", p).await
    }

    pub async fn send_video(&self, token: &str, p: &SendMediaParams) -> AppResult<TelegramMessage> {
        self.send_media_json(token, "sendVideo", "video", p).await
    }

    pub async fn send_audio(&self, token: &str, p: &SendMediaParams) -> AppResult<TelegramMessage> {
        self.send_media_json(token, "sendAudio", "audio", p).await
    }

    pub async fn send_animation(&self, token: &str, p: &SendMediaParams) -> AppResult<TelegramMessage> {
        self.send_media_json(token, "sendAnimation", "animation", p).await
    }

    /// Upload file mentah (multipart) — dipakai endpoint /api/messages/send-media.
    pub async fn send_media_bytes(
        &self,
        token: &str,
        method: &str,
        field: &str,
        chat_id: i64,
        bytes: Vec<u8>,
        filename: String,
        caption: Option<String>,
    ) -> AppResult<TelegramMessage> {
        let part = Part::bytes(bytes).file_name(filename);
        let mut form = Form::new()
            .text("chat_id", chat_id.to_string())
            .part(field.to_string(), part);
        if let Some(c) = caption {
            form = form.text("caption", c);
        }
        self.call_multipart(token, method, form).await
    }

    pub async fn send_location(
        &self,
        token: &str,
        chat_id: i64,
        latitude: f64,
        longitude: f64,
    ) -> AppResult<TelegramMessage> {
        self.call(
            token,
            "sendLocation",
            &serde_json::json!({"chat_id": chat_id, "latitude": latitude, "longitude": longitude}),
        )
        .await
    }

    pub async fn send_contact(
        &self,
        token: &str,
        chat_id: i64,
        phone_number: &str,
        first_name: &str,
    ) -> AppResult<TelegramMessage> {
        self.call(
            token,
            "sendContact",
            &serde_json::json!({"chat_id": chat_id, "phone_number": phone_number, "first_name": first_name}),
        )
        .await
    }

    /// Kirim album media (multipart: field `media` berisi JSON array
    /// `InputMediaPhoto`/`InputMediaVideo`, tiap item `media` berupa file_id/URL).
    pub async fn send_media_group(
        &self,
        token: &str,
        chat_id: i64,
        media: &[serde_json::Value],
    ) -> AppResult<Vec<TelegramMessage>> {
        let form = Form::new()
            .text("chat_id", chat_id.to_string())
            .text("media", serde_json::to_string(media).unwrap_or_else(|_| "[]".to_string()));
        self.call_multipart(token, "sendMediaGroup", form).await
    }

    // --- Updates / webhook ---

    pub async fn get_updates(
        &self,
        token: &str,
        offset: Option<i64>,
        timeout_secs: i64,
    ) -> AppResult<Vec<Update>> {
        let mut body = serde_json::json!({
            "timeout": timeout_secs,
            "allowed_updates": ["message", "edited_message", "channel_post", "callback_query", "my_chat_member"],
        });
        if let Some(o) = offset {
            body["offset"] = serde_json::Value::from(o);
        }
        self.call(token, "getUpdates", &body).await
    }

    pub async fn set_webhook(&self, token: &str, url: &str) -> AppResult<bool> {
        self.call(
            token,
            "setWebhook",
            &serde_json::json!({
                "url": url,
                "allowed_updates": ["message", "edited_message", "channel_post", "callback_query", "my_chat_member"],
            }),
        )
        .await
    }

    pub async fn delete_webhook(&self, token: &str, drop_pending: bool) -> AppResult<bool> {
        self.call(
            token,
            "deleteWebhook",
            &serde_json::json!({"drop_pending_updates": drop_pending}),
        )
        .await
    }

    pub async fn get_webhook_info(&self, token: &str) -> AppResult<WebhookInfo> {
        self.call(token, "getWebhookInfo", &serde_json::json!({})).await
    }

    // --- Chat & member ---

    pub async fn get_chat(&self, token: &str, chat_id: i64) -> AppResult<TelegramChat> {
        self.call(token, "getChat", &serde_json::json!({"chat_id": chat_id}))
            .await
    }

    pub async fn get_chat_member(
        &self,
        token: &str,
        chat_id: i64,
        user_id: i64,
    ) -> AppResult<ChatMember> {
        self.call(
            token,
            "getChatMember",
            &serde_json::json!({"chat_id": chat_id, "user_id": user_id}),
        )
        .await
    }

    /// Apakah bot sendiri admin/creator di chat ini (dipakai sebelum moderasi).
    pub async fn is_bot_admin(&self, token: &str, chat_id: i64, bot_user_id: i64) -> bool {
        match self.get_chat_member(token, chat_id, bot_user_id).await {
            Ok(m) => m.status == "administrator" || m.status == "creator",
            Err(_) => false,
        }
    }

    // --- Moderasi (hanya dipanggil setelah cek admin) ---

    pub async fn delete_message(
        &self,
        token: &str,
        chat_id: i64,
        message_id: i64,
    ) -> AppResult<bool> {
        self.call(
            token,
            "deleteMessage",
            &serde_json::json!({"chat_id": chat_id, "message_id": message_id}),
        )
        .await
    }

    pub async fn ban_chat_member(&self, token: &str, chat_id: i64, user_id: i64) -> AppResult<bool> {
        self.call(
            token,
            "banChatMember",
            &serde_json::json!({"chat_id": chat_id, "user_id": user_id}),
        )
        .await
    }

    pub async fn unban_chat_member(
        &self,
        token: &str,
        chat_id: i64,
        user_id: i64,
    ) -> AppResult<bool> {
        self.call(
            token,
            "unbanChatMember",
            &serde_json::json!({"chat_id": chat_id, "user_id": user_id, "only_if_banned": true}),
        )
        .await
    }

    pub async fn restrict_chat_member(
        &self,
        token: &str,
        chat_id: i64,
        user_id: i64,
        can_send_messages: bool,
    ) -> AppResult<bool> {
        self.call(
            token,
            "restrictChatMember",
            &serde_json::json!({
                "chat_id": chat_id,
                "user_id": user_id,
                "permissions": {"can_send_messages": can_send_messages},
            }),
        )
        .await
    }

    // --- Commands ---

    pub async fn set_my_commands(
        &self,
        token: &str,
        commands: &[TgBotCommand],
    ) -> AppResult<bool> {
        self.call(token, "setMyCommands", &serde_json::json!({"commands": commands}))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_get_me_response() {
        let json = r#"{
            "ok": true,
            "result": {
                "id": 123456789,
                "is_bot": true,
                "first_name": "TestBot",
                "username": "test_bot",
                "can_join_groups": true,
                "can_read_all_group_messages": false,
                "supports_inline_queries": false
            }
        }"#;
        let parsed: TelegramResponse<TelegramUser> = serde_json::from_str(json).unwrap();
        assert!(parsed.ok);
        let user = parsed.result.unwrap();
        assert_eq!(user.id, 123456789);
        assert!(user.is_bot);
        assert_eq!(user.username.as_deref(), Some("test_bot"));
    }

    #[test]
    fn parse_get_me_error_response() {
        let json = r#"{"ok": false, "error_code": 401, "description": "Unauthorized"}"#;
        let parsed: TelegramResponse<TelegramUser> = serde_json::from_str(json).unwrap();
        assert!(!parsed.ok);
        assert_eq!(parsed.error_code, Some(401));
        assert_eq!(parsed.description.as_deref(), Some("Unauthorized"));
    }

    #[test]
    fn parse_update_with_text_message() {
        let json = r#"{
            "update_id": 10000,
            "message": {
                "message_id": 1,
                "from": {"id": 111, "is_bot": false, "first_name": "Astr", "username": "astr"},
                "chat": {"id": 111, "type": "private", "first_name": "Astr", "username": "astr"},
                "date": 1793412000,
                "text": "/start halo"
            }
        }"#;
        let update: Update = serde_json::from_str(json).unwrap();
        assert_eq!(update.update_id, 10000);
        let msg = update.message.unwrap();
        assert_eq!(msg.text.as_deref(), Some("/start halo"));
        assert_eq!(msg.chat.chat_type, "private");
        assert!(!msg.from.unwrap().is_bot);
    }

    #[test]
    fn parse_update_with_new_chat_member() {
        let json = r#"{
            "update_id": 10001,
            "message": {
                "message_id": 2,
                "from": {"id": 222, "is_bot": false, "first_name": "Budi"},
                "chat": {"id": -100123, "type": "supergroup", "title": "Grup Test"},
                "date": 1793412001,
                "new_chat_members": [{"id": 333, "is_bot": false, "first_name": "Citra"}]
            }
        }"#;
        let update: Update = serde_json::from_str(json).unwrap();
        let msg = update.message.unwrap();
        let members = msg.new_chat_members.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].first_name, "Citra");
    }
}
