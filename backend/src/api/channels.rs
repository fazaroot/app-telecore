//! Channel Posting (v1.2).
//!
//! - Daftar channel dari `bot_chats` tipe `channel`.
//! - Posting langsung: kirim pesan ke channel via Telegram + simpan ke messages.
//! - Posting terjadwal: buat `scheduled_task` one-time dengan
//!   `target_type='channel'` / `target_id=<chat_id>`; scheduler yang ada
//!   mengeksekusinya pada `run_at` (kirim ke `chat_id` task).

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::bot::get_bot_token;
use crate::bot::telegram::SendMessageParams;
use crate::error::{AppError, AppResult, Envelope};
use crate::models::{new_id, now_rfc3339, Message};
use crate::state::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ChannelInfo {
    pub bot_id: String,
    pub bot_username: Option<String>,
    pub chat_id: i64,
    pub title: Option<String>,
    pub username: Option<String>,
}

pub async fn list_channels(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<Vec<ChannelInfo>>>> {
    let channels = sqlx::query_as::<_, ChannelInfo>(
        "SELECT c.bot_id AS bot_id, b.username AS bot_username, c.chat_id AS chat_id, \
         c.title AS title, c.username AS username \
         FROM bot_chats c JOIN bots b ON b.id = c.bot_id \
         WHERE c.type = 'channel' ORDER BY c.title, c.chat_id",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Envelope::ok(channels))
}

#[derive(Debug, Deserialize)]
pub struct PostChannelRequest {
    pub bot_id: String,
    pub text: String,
    #[serde(default, deserialize_with = "crate::models::de_parse_mode")]
    pub parse_mode: Option<String>,
}

pub async fn post_channel(
    State(state): State<AppState>,
    Path(chat_id): Path<i64>,
    Json(req): Json<PostChannelRequest>,
) -> AppResult<Json<Envelope<Message>>> {
    if req.text.trim().is_empty() {
        return Err(AppError::Validation("text tidak boleh kosong".to_string()));
    }
    if let Some(mode) = &req.parse_mode {
        if !matches!(mode.as_str(), "Markdown" | "MarkdownV2" | "HTML") {
            return Err(AppError::Validation(
                "parse_mode harus Markdown, MarkdownV2, atau HTML".to_string(),
            ));
        }
    }
    let token = get_bot_token(&state, &req.bot_id).await?;
    let sent = state
        .telegram
        .send_message(&token, &SendMessageParams {
            chat_id,
            text: req.text.clone(),
            parse_mode: req.parse_mode.clone(),
            ..Default::default()
        })
        .await?;
    let id = new_id();
    let now = now_rfc3339();
    sqlx::query(
        "INSERT INTO messages (id, bot_id, chat_id, telegram_message_id, direction, \
         message_type, content, status, parse_mode, is_read, created_at, updated_at) \
         VALUES (?, ?, ?, ?, 'outbound', 'text', ?, 'sent', ?, 1, ?, ?)",
    )
    .bind(&id)
    .bind(&req.bot_id)
    .bind(chat_id)
    .bind(sent.message_id)
    .bind(&req.text)
    .bind(req.parse_mode.as_deref())
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    let msg = sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Envelope::ok(msg))
}

#[derive(Debug, Deserialize)]
pub struct ScheduleChannelRequest {
    pub bot_id: String,
    pub text: String,
    /// ISO8601 / RFC3339, mis. "2026-11-26T09:00:00Z".
    pub run_at: String,
    #[serde(default, deserialize_with = "crate::models::de_parse_mode")]
    pub parse_mode: Option<String>,
}

pub async fn schedule_channel(
    State(state): State<AppState>,
    Path(chat_id): Path<i64>,
    Json(req): Json<ScheduleChannelRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    if req.text.trim().is_empty() {
        return Err(AppError::Validation("text tidak boleh kosong".to_string()));
    }
    let run_at = chrono::DateTime::parse_from_rfc3339(&req.run_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|_| {
            AppError::Validation(format!(
                "run_at tidak valid (harus RFC3339): '{}'",
                req.run_at
            ))
        })?;
    if run_at <= chrono::Utc::now() {
        return Err(AppError::Validation(
            "run_at harus di masa depan".to_string(),
        ));
    }
    // Bot + channel harus dikenal.
    let _ = get_bot_token(&state, &req.bot_id).await?;
    let now = now_rfc3339();
    let id = new_id();
    // parse_mode tidak disimpan di scheduled_tasks v1 — konten dikirim
    // sebagai teks biasa oleh scheduler; dicatat di deskripsi.
    let description = match &req.parse_mode {
        Some(m) => format!("Channel post terjadwal (parse_mode: {m})"),
        None => "Channel post terjadwal".to_string(),
    };
    sqlx::query(
        "INSERT INTO scheduled_tasks (id, bot_id, name, description, task_type, chat_id, \
         message_content, timezone, next_run_at, target_type, target_id, created_at, updated_at) \
         VALUES (?, ?, ?, ?, 'one-time', ?, ?, 'UTC', ?, 'channel', ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.bot_id)
    .bind(format!("Channel post {chat_id}"))
    .bind(&description)
    .bind(chat_id)
    .bind(&req.text)
    .bind(run_at.to_rfc3339())
    .bind(chat_id.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(json!({
        "scheduled_task_id": id,
        "chat_id": chat_id,
        "run_at": run_at.to_rfc3339(),
    })))
}
