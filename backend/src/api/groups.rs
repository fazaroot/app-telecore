//! Grup: list, detail, settings, moderation log, aksi moderasi.
//!
//! Aksi moderasi HANYA dijalankan bila bot adalah admin/creator di grup
//! (dicek via `getChatMember`); bila bukan admin, dikembalikan error 403
//! yang jelas.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::bot::get_bot_token;
use crate::error::{AppError, AppResult, Envelope};
use crate::models::{now_rfc3339, ActivityLog, Bot, BotChat, Paginated, Pagination};
use crate::services::log_activity;
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct ListGroupsQuery {
    pub bot_id: Option<String>,
    #[serde(flatten)]
    pub pagination: Pagination,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGroupSettingsRequest {
    pub settings: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ModerateRequest {
    /// ban | unban | mute | unmute | delete_message
    pub action: String,
    pub user_id: Option<i64>,
    pub message_id: Option<i64>,
    pub reason: Option<String>,
}

async fn fetch_chat(state: &AppState, id: &str) -> AppResult<BotChat> {
    sqlx::query_as::<_, BotChat>("SELECT * FROM bot_chats WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("grup {id} tidak ditemukan")))
}

pub async fn list_groups(
    State(state): State<AppState>,
    Query(q): Query<ListGroupsQuery>,
) -> AppResult<Json<Envelope<Paginated<BotChat>>>> {
    let (page, per_page) = q.pagination.normalized();
    let offset = q.pagination.offset();
    let (items, total) = match &q.bot_id {
        Some(bot_id) => (
            sqlx::query_as::<_, BotChat>(
                "SELECT * FROM bot_chats WHERE bot_id = ? AND type IN ('group', 'supergroup') \
                 ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(bot_id)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM bot_chats WHERE bot_id = ? AND type IN ('group', 'supergroup')",
            )
            .bind(bot_id)
            .fetch_one(&state.pool)
            .await?,
        ),
        None => (
            sqlx::query_as::<_, BotChat>(
                "SELECT * FROM bot_chats WHERE type IN ('group', 'supergroup') \
                 ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM bot_chats WHERE type IN ('group', 'supergroup')",
            )
            .fetch_one(&state.pool)
            .await?,
        ),
    };
    Ok(Envelope::ok(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}

pub async fn get_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<BotChat>>> {
    Ok(Envelope::ok(fetch_chat(&state, &id).await?))
}

pub async fn update_group_settings(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateGroupSettingsRequest>,
) -> AppResult<Json<Envelope<BotChat>>> {
    let mut chat = fetch_chat(&state, &id).await?;
    if !req.settings.is_object() {
        return Err(AppError::Validation("settings harus berupa objek JSON".to_string()));
    }
    // Merge dangkal: kunci baru menimpa kunci lama, kunci lain dipertahankan.
    let mut current: serde_json::Value =
        serde_json::from_str(&chat.settings).unwrap_or(json!({}));
    if let (Some(cur), Some(new)) = (current.as_object_mut(), req.settings.as_object()) {
        for (k, v) in new {
            cur.insert(k.clone(), v.clone());
        }
    } else {
        current = req.settings;
    }
    chat.settings = current.to_string();
    chat.updated_at = now_rfc3339();
    sqlx::query("UPDATE bot_chats SET settings = ?, updated_at = ? WHERE id = ?")
        .bind(&chat.settings)
        .bind(&chat.updated_at)
        .bind(&chat.id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(chat))
}

pub async fn moderation_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<Vec<ActivityLog>>>> {
    let chat = fetch_chat(&state, &id).await?;
    let logs = sqlx::query_as::<_, ActivityLog>(
        "SELECT * FROM activity_logs WHERE action = 'moderation' AND entity_id = ? \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(chat.chat_id.to_string())
    .fetch_all(&state.pool)
    .await?;
    Ok(Envelope::ok(logs))
}

/// Aksi moderasi — hanya bila bot admin di grup.
pub async fn moderate(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ModerateRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let chat = fetch_chat(&state, &id).await?;
    let mut bot = sqlx::query_as::<_, Bot>("SELECT * FROM bots WHERE id = ?")
        .bind(&chat.bot_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bot {} tidak ditemukan", chat.bot_id)))?;
    let token = get_bot_token(&state, &bot.id).await?;

    // Butuh telegram_id bot sendiri untuk cek getChatMember; isi dari getMe
    // bila belum tersimpan.
    if bot.telegram_id.is_none() {
        let me = state.telegram.get_me(&token).await?;
        bot.telegram_id = Some(me.id);
        let _ = sqlx::query("UPDATE bots SET telegram_id = ? WHERE id = ?")
            .bind(me.id)
            .bind(&bot.id)
            .execute(&state.pool)
            .await;
    }
    let bot_tg_id = bot.telegram_id.unwrap();

    // Cek admin LANGSUNG ke Telegram (bukan hanya flag tersimpan).
    let is_admin = state
        .telegram
        .is_bot_admin(&token, chat.chat_id, bot_tg_id)
        .await;
    let _ = sqlx::query("UPDATE bot_chats SET is_bot_admin = ? WHERE id = ?")
        .bind(if is_admin { 1 } else { 0 })
        .bind(&chat.id)
        .execute(&state.pool)
        .await;
    if !is_admin {
        return Err(AppError::Forbidden(
            "Bot bukan admin di grup ini — aksi moderasi tidak dapat dilakukan. \
             Jadikan bot sebagai admin grup terlebih dahulu."
                .to_string(),
        ));
    }

    match req.action.as_str() {
        "ban" => {
            let user_id = req
                .user_id
                .ok_or_else(|| AppError::Validation("user_id wajib untuk ban".to_string()))?;
            state
                .telegram
                .ban_chat_member(&token, chat.chat_id, user_id)
                .await?;
        }
        "unban" => {
            let user_id = req
                .user_id
                .ok_or_else(|| AppError::Validation("user_id wajib untuk unban".to_string()))?;
            state
                .telegram
                .unban_chat_member(&token, chat.chat_id, user_id)
                .await?;
        }
        "mute" => {
            let user_id = req
                .user_id
                .ok_or_else(|| AppError::Validation("user_id wajib untuk mute".to_string()))?;
            state
                .telegram
                .restrict_chat_member(&token, chat.chat_id, user_id, false)
                .await?;
        }
        "unmute" => {
            let user_id = req
                .user_id
                .ok_or_else(|| AppError::Validation("user_id wajib untuk unmute".to_string()))?;
            state
                .telegram
                .restrict_chat_member(&token, chat.chat_id, user_id, true)
                .await?;
        }
        "delete_message" => {
            let message_id = req.message_id.ok_or_else(|| {
                AppError::Validation("message_id wajib untuk delete_message".to_string())
            })?;
            state
                .telegram
                .delete_message(&token, chat.chat_id, message_id)
                .await?;
        }
        other => {
            return Err(AppError::Validation(format!(
                "action moderasi tidak dikenal: '{other}' (ban, unban, mute, unmute, delete_message)"
            )));
        }
    }

    log_activity(
        &state,
        Some(&chat.bot_id),
        "moderation",
        Some("moderation"),
        Some(&chat.chat_id.to_string()),
        json!({
            "chat_id": chat.chat_id,
            "user_id": req.user_id,
            "message_id": req.message_id,
            "action": req.action,
            "reason": req.reason,
            "via": "api",
        }),
        "warning",
    )
    .await;
    Ok(Envelope::ok(json!({
        "success": true,
        "action": req.action,
        "chat_id": chat.chat_id,
    })))
}
