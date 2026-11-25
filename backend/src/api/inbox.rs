//! Live Chat Inbox (v1.2): daftar percakapan, isi percakapan, balas.
//!
//! Percakapan dikelompokkan dari `messages` + `bot_users`/`bot_chats`.
//! Pesan masuk baru tersimpan dengan `is_read = 0` (lihat
//! update_processor); membuka percakapan menandai semuanya terbaca.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::bot::get_bot_token;
use crate::bot::telegram::SendMessageParams;
use crate::error::{AppError, AppResult, Envelope};
use crate::models::{new_id, now_rfc3339, Message};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct Conversation {
    pub bot_id: String,
    pub chat_id: i64,
    pub chat_type: String,
    pub title: Option<String>,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_message_text: Option<String>,
    pub last_message_at: String,
    pub unread_count: i64,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListInboxQuery {
    pub bot_id: Option<String>,
}

pub async fn list_inbox(
    State(state): State<AppState>,
    Query(q): Query<ListInboxQuery>,
) -> AppResult<Json<Envelope<Vec<Conversation>>>> {
    // Ambil pesan terbaru dulu (maks 1000) lalu agregasi di Rust — SQLite
    // tanpa window function yang nyaman untuk "latest per group".
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT * FROM messages WHERE 1=1",
    );
    if let Some(b) = &q.bot_id {
        qb.push(" AND bot_id = ").push_bind(b);
    }
    qb.push(" ORDER BY created_at DESC, rowid DESC LIMIT 1000");
    let msgs = qb.build_query_as::<Message>().fetch_all(&state.pool).await?;

    // Unread per (bot_id, chat_id).
    let unread: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT bot_id, chat_id, COUNT(*) FROM messages \
         WHERE direction = 'inbound' AND is_read = 0 GROUP BY bot_id, chat_id",
    )
    .fetch_all(&state.pool)
    .await?;
    let unread_map: HashMap<(String, i64), i64> = unread
        .into_iter()
        .map(|(b, c, n)| ((b, c), n))
        .collect();

    let mut convos: Vec<Conversation> = Vec::new();
    let mut seen: HashMap<(String, i64), usize> = HashMap::new();
    for m in &msgs {
        let key = (m.bot_id.clone(), m.chat_id);
        if seen.contains_key(&key) {
            continue;
        }
        // Judul: bot_chats.title, fallback nama user pengirim terakhir.
        let chat_row: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
            "SELECT title, username, type FROM bot_chats WHERE bot_id = ? AND chat_id = ?",
        )
        .bind(&m.bot_id)
        .bind(m.chat_id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
        let (mut title, mut username, chat_type) = match chat_row {
            Some((t, u, ty)) => (t, u, ty),
            None => (None, None, "private".to_string()),
        };
        let mut first_name: Option<String> = None;
        if chat_type == "private" {
            // Nama user dari bot_users berdasarkan from_user_id pesan terakhir.
            if let Some(uid) = m.from_user_id {
                let user: Option<(Option<String>, Option<String>, Option<String>)> =
                    sqlx::query_as(
                        "SELECT username, first_name, last_name FROM bot_users \
                         WHERE bot_id = ? AND telegram_user_id = ?",
                    )
                    .bind(&m.bot_id)
                    .bind(uid)
                    .fetch_optional(&state.pool)
                    .await
                    .ok()
                    .flatten();
                if let Some((u, f, l)) = user {
                    if username.is_none() {
                        username = u;
                    }
                    first_name = f.clone();
                    title = Some(match (f, l) {
                        (Some(f), Some(l)) => format!("{f} {l}"),
                        (Some(f), None) => f,
                        (None, Some(l)) => l,
                        (None, None) => m.chat_id.to_string(),
                    });
                }
            }
        }
        if title.is_none() {
            title = Some(m.chat_id.to_string());
        }
        seen.insert(key.clone(), convos.len());
        convos.push(Conversation {
            bot_id: m.bot_id.clone(),
            chat_id: m.chat_id,
            chat_type,
            title,
            username,
            first_name,
            last_message_text: m.content.clone(),
            last_message_at: m.created_at.clone(),
            unread_count: *unread_map.get(&key).unwrap_or(&0),
        });
    }
    // Sudah urut last_message_at DESC karena pesan diambil DESC.
    Ok(Envelope::ok(convos))
}

#[derive(Debug, Serialize)]
pub struct ConversationDetail {
    pub chat: Conversation,
    pub messages: Vec<Message>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ConversationQuery {
    pub bot_id: Option<String>,
}

pub async fn get_conversation(
    State(state): State<AppState>,
    Path(chat_id): Path<i64>,
    Query(q): Query<ConversationQuery>,
) -> AppResult<Json<Envelope<ConversationDetail>>> {
    // Tentukan bot: dari query, atau bot dengan pesan terbaru di chat ini.
    let bot_id: Option<String> = match q.bot_id {
        Some(b) => Some(b),
        None => sqlx::query_scalar::<_, String>(
            "SELECT bot_id FROM messages WHERE chat_id = ? ORDER BY created_at DESC, rowid DESC LIMIT 1",
        )
        .bind(chat_id)
        .fetch_optional(&state.pool)
        .await?,
    };
    let Some(bot_id) = bot_id else {
        return Err(AppError::NotFound(format!(
            "percakapan {chat_id} tidak ditemukan"
        )));
    };
    // Tandai semua pesan masuk chat ini sebagai terbaca.
    sqlx::query(
        "UPDATE messages SET is_read = 1 WHERE bot_id = ? AND chat_id = ? AND direction = 'inbound'",
    )
    .bind(&bot_id)
    .bind(chat_id)
    .execute(&state.pool)
    .await?;
    let messages = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE bot_id = ? AND chat_id = ? ORDER BY created_at ASC, rowid ASC LIMIT 500",
    )
    .bind(&bot_id)
    .bind(chat_id)
    .fetch_all(&state.pool)
    .await?;
    if messages.is_empty() {
        return Err(AppError::NotFound(format!(
            "percakapan {chat_id} tidak ditemukan"
        )));
    }
    let last = messages.last().unwrap();
    let chat_row: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT title, username, type FROM bot_chats WHERE bot_id = ? AND chat_id = ?",
    )
    .bind(&bot_id)
    .bind(chat_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let (title, username, chat_type) = match chat_row {
        Some((t, u, ty)) => (t, u, ty),
        None => (None, None, "private".to_string()),
    };
    let chat = Conversation {
        bot_id: bot_id.clone(),
        chat_id,
        chat_type,
        title: title.or_else(|| Some(chat_id.to_string())),
        username,
        first_name: None,
        last_message_text: last.content.clone(),
        last_message_at: last.created_at.clone(),
        unread_count: 0,
    };
    Ok(Envelope::ok(ConversationDetail { chat, messages }))
}

#[derive(Debug, Deserialize)]
pub struct ReplyRequest {
    pub bot_id: Option<String>,
    pub text: String,
    #[serde(default, deserialize_with = "crate::models::de_parse_mode")]
    pub parse_mode: Option<String>,
}

pub async fn reply_conversation(
    State(state): State<AppState>,
    Path(chat_id): Path<i64>,
    Json(req): Json<ReplyRequest>,
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
    let bot_id: Option<String> = match req.bot_id {
        Some(b) => Some(b),
        None => sqlx::query_scalar::<_, String>(
            "SELECT bot_id FROM messages WHERE chat_id = ? ORDER BY created_at DESC, rowid DESC LIMIT 1",
        )
        .bind(chat_id)
        .fetch_optional(&state.pool)
        .await?,
    };
    let Some(bot_id) = bot_id else {
        return Err(AppError::NotFound(format!(
            "percakapan {chat_id} tidak ditemukan"
        )));
    };
    let token = get_bot_token(&state, &bot_id).await?;
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
    .bind(&bot_id)
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
