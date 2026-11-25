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
    /// v1.2: pembayaran sukses (successful_payment) — isi mentah JSON.
    #[serde(default)]
    pub successful_payment: Option<serde_json::Value>,
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
    /// v1.2: pre-checkout query pembayaran.
    #[serde(default)]
    pub pre_checkout_query: Option<serde_json::Value>,
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

#[derive(Debug, Clone, Serialize, Default)]
pub struct SendPollParams {
    pub chat_id: i64,
    pub question: String,
    pub options: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_anonymous: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_multiple_answers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub poll_type: Option<String>, // "regular" | "quiz"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correct_option_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SendInvoiceParams {
    pub chat_id: i64,
    pub title: String,
    pub description: String,
    pub payload: String,
    /// WAJIB "" untuk Telegram Stars; v1.2 tidak menyimpan provider token
    /// nyata (lihat README v1.2).
    pub provider_token: String,
    pub currency: String,
    pub prices: Vec<serde_json::Value>, // [{label, amount}]
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

    /// Konstruktor dengan base URL kustom — dipakai test dengan server
    /// Telegram palsu lokal. Produksi selalu memakai `new()`.
    #[cfg(test)]
    pub fn with_base_url(base_url: String) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("TeleCore/0.1 (+https://github.com/fazaroot)")
            .build()?;
        Ok(Self { http, base_url })
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
            "allowed_updates": ["message", "edited_message", "channel_post", "callback_query", "my_chat_member", "pre_checkout_query"],
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
                "allowed_updates": ["message", "edited_message", "channel_post", "callback_query", "my_chat_member", "pre_checkout_query"],
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

    /// Jumlah anggota chat (getChatMemberCount). Gagal bila bot bukan
    /// anggota chat / tidak punya akses — pemanggil memakai fallback
    /// `member_count` tersimpan.
    pub async fn get_chat_member_count(&self, token: &str, chat_id: i64) -> AppResult<i64> {
        self.call(
            token,
            "getChatMemberCount",
            &serde_json::json!({"chat_id": chat_id}),
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
        self.restrict_chat_member_until(token, chat_id, user_id, can_send_messages, None)
            .await
    }

    /// Restrict dengan `until_date` (Unix timestamp, detik) opsional.
    /// `until_date = None` = tanpa batas waktu (dipakai unmute/izin penuh
    /// lewat `can_send_messages = true`).
    pub async fn restrict_chat_member_until(
        &self,
        token: &str,
        chat_id: i64,
        user_id: i64,
        can_send_messages: bool,
        until_date: Option<i64>,
    ) -> AppResult<bool> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "user_id": user_id,
            "permissions": {"can_send_messages": can_send_messages},
        });
        if let Some(until) = until_date {
            body["until_date"] = serde_json::Value::from(until);
        }
        self.call(token, "restrictChatMember", &body).await
    }

    // --- Poll, sticker & pembayaran (v1.2) ---

    pub async fn send_poll(
        &self,
        token: &str,
        params: &SendPollParams,
    ) -> AppResult<TelegramMessage> {
        self.call(token, "sendPoll", params).await
    }

    pub async fn send_sticker(
        &self,
        token: &str,
        chat_id: i64,
        sticker: &str,
    ) -> AppResult<TelegramMessage> {
        self.call(
            token,
            "sendSticker",
            &serde_json::json!({"chat_id": chat_id, "sticker": sticker}),
        )
        .await
    }

    /// Kirim invoice. Untuk Stars `provider_token` HARUS "" (Telegram
    /// menolak token lain untuk XTR).
    pub async fn send_invoice(
        &self,
        token: &str,
        params: &SendInvoiceParams,
    ) -> AppResult<TelegramMessage> {
        self.call(token, "sendInvoice", params).await
    }

    pub async fn answer_pre_checkout_query(
        &self,
        token: &str,
        pre_checkout_query_id: &str,
        ok: bool,
        error_message: Option<&str>,
    ) -> AppResult<bool> {
        let mut body = serde_json::json!({
            "pre_checkout_query_id": pre_checkout_query_id,
            "ok": ok,
        });
        if let Some(msg) = error_message {
            body["error_message"] = serde_json::Value::String(msg.to_string());
        }
        self.call(token, "answerPreCheckoutQuery", &body).await
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

    /// Server Telegram palsu (TCP lokal): membalas setiap request dengan
    /// `canned` JSON dan merekam request mentah ke channel. Dipakai untuk
    /// menguji method client tanpa menyentuh api.telegram.org.
    fn fake_telegram_server(
        canned: &str,
    ) -> (String, std::sync::mpsc::Receiver<String>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let canned = canned.to_string();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            head.push_str(&line);
                            if line == "\r\n" {
                                break;
                            }
                        }
                    }
                }
                let content_len = head
                    .lines()
                    .find_map(|l| {
                        l.strip_prefix("Content-Length:")
                            .or_else(|| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let mut body = vec![0u8; content_len];
                if content_len > 0 {
                    let _ = reader.read_exact(&mut body);
                }
                let raw = format!("{head}{}", String::from_utf8_lossy(&body));
                let _ = tx.send(raw);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    canned.len(),
                    canned
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        });
        (format!("http://{addr}"), rx)
    }

    #[tokio::test]
    async fn get_chat_member_count_against_fake_server() {
        let (base, rx) = fake_telegram_server(r#"{"ok":true,"result":42}"#);
        let client = TelegramClient::with_base_url(base).unwrap();
        let count = client.get_chat_member_count("FAKE", -100123).await.unwrap();
        assert_eq!(count, 42);
        let req = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(req.contains("getChatMemberCount"));
        assert!(req.contains("-100123"));
    }

    #[tokio::test]
    async fn restrict_with_until_date_sends_until_date() {
        let (base, rx) = fake_telegram_server(r#"{"ok":true,"result":true}"#);
        let client = TelegramClient::with_base_url(base).unwrap();
        let ok = client
            .restrict_chat_member_until("FAKE", -100123, 777, false, Some(1793412000))
            .await
            .unwrap();
        assert!(ok);
        let req = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(req.contains("restrictChatMember"));
        assert!(req.contains("1793412000"));
        assert!(req.contains("can_send_messages"));
    }

    #[tokio::test]
    async fn telegram_api_error_surfaces_as_error() {
        let (base, _rx) =
            fake_telegram_server(r#"{"ok":false,"error_code":403,"description":"Forbidden: bot is not a member"}"#);
        let client = TelegramClient::with_base_url(base).unwrap();
        let err = client.get_chat_member_count("FAKE", -100123).await.unwrap_err();
        assert!(err.to_string().contains("403"));
    }
}
