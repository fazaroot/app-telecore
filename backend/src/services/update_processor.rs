//! Pemrosesan update Telegram — dipakai BERSAMA oleh webhook dan long polling
//! agar perilakunya identik:
//!
//! 1. Abaikan pesan dari bot (loop guard, termasuk bot sendiri).
//! 2. Upsert user & chat, simpan pesan masuk.
//! 3. Moderasi grup dasar (keyword/link/flood) — eksekusi HANYA bila bot admin,
//!    bila bukan admin hanya dicatat di activity log.
//! 4. Auto-reply command kustom + jalankan automation engine.
//! 5. Broadcast event WebSocket.

use crate::automation::{render_template, run_workflows};
use crate::bot::get_bot_token;
use crate::bot::telegram::{SendMessageParams, TelegramMessage, Update};
use crate::models::{new_id, now_rfc3339, BotChat, BotCommand};
use crate::services::outgoing_webhooks::dispatch_outgoing_event;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Activity log helper (dipakai banyak modul)
// ---------------------------------------------------------------------------

pub async fn log_activity(
    state: &AppState,
    bot_id: Option<&str>,
    action: &str,
    entity_type: Option<&str>,
    entity_id: Option<&str>,
    details: serde_json::Value,
    level: &str,
) {
    let _ = sqlx::query(
        "INSERT INTO activity_logs (id, bot_id, action, entity_type, entity_id, details, level, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(bot_id)
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(details.to_string())
    .bind(level)
    .bind(now_rfc3339())
    .execute(&state.pool)
    .await;
}

// ---------------------------------------------------------------------------
// Pemrosesan update
// ---------------------------------------------------------------------------

pub async fn process_update(state: &AppState, bot_id: &str, update: &Update) {
    let Some(msg) = update
        .message
        .as_ref()
        .or(update.edited_message.as_ref())
        .or(update.channel_post.as_ref())
    else {
        return;
    };

    // Loop guard: abaikan pesan dari bot (termasuk bot sendiri).
    if msg.from.as_ref().map(|u| u.is_bot).unwrap_or(false) {
        return;
    }

    let now = now_rfc3339();
    let chat = &msg.chat;
    let from = msg.from.as_ref();
    let text = msg.text.clone().or_else(|| msg.caption.clone()).unwrap_or_default();

    // --- Upsert user ---
    let mut is_new_user = false;
    if let Some(user) = from {
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM bot_users WHERE bot_id = ? AND telegram_user_id = ?",
        )
        .bind(bot_id)
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
        is_new_user = existing.is_none();
        let _ = sqlx::query(
            "INSERT INTO bot_users (id, bot_id, telegram_user_id, username, first_name, last_name, \
             language_code, is_bot, last_seen_at, message_count, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, 1, ?, ?) \
             ON CONFLICT(bot_id, telegram_user_id) DO UPDATE SET \
             username = excluded.username, first_name = excluded.first_name, \
             last_name = excluded.last_name, language_code = excluded.language_code, \
             last_seen_at = excluded.last_seen_at, \
             message_count = message_count + 1, updated_at = excluded.updated_at",
        )
        .bind(new_id())
        .bind(bot_id)
        .bind(user.id)
        .bind(user.username.as_deref())
        .bind(user.first_name.as_str())
        .bind(user.last_name.as_deref())
        .bind(user.language_code.as_deref())
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await;
    }

    // --- Upsert chat ---
    let _ = sqlx::query(
        "INSERT INTO bot_chats (id, bot_id, chat_id, type, title, username, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(bot_id, chat_id) DO UPDATE SET \
         title = excluded.title, username = excluded.username, updated_at = excluded.updated_at",
    )
    .bind(new_id())
    .bind(bot_id)
    .bind(chat.id)
    .bind(chat.chat_type.as_str())
    .bind(chat.title.as_deref())
    .bind(chat.username.as_deref())
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await;

    // --- Simpan pesan masuk ---
    let message_type = if msg.photo.is_some() {
        "photo"
    } else if msg.new_chat_members.is_some() {
        "new_chat_members"
    } else {
        "text"
    };
    let _ = sqlx::query(
        "INSERT INTO messages (id, bot_id, chat_id, telegram_message_id, direction, \
         message_type, content, status, from_user_id, created_at, updated_at) \
         VALUES (?, ?, ?, ?, 'inbound', ?, ?, 'received', ?, ?, ?)",
    )
    .bind(new_id())
    .bind(bot_id)
    .bind(chat.id)
    .bind(msg.message_id)
    .bind(message_type)
    .bind(if text.is_empty() { None } else { Some(text.clone()) })
    .bind(from.map(|u| u.id))
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await;

    // --- Outgoing webhook: message.inbound (non-blocking, spawn) ---
    dispatch_outgoing_event(
        state,
        "message.inbound",
        Some(bot_id),
        serde_json::json!({
            "chat_id": chat.id,
            "from_user_id": from.map(|u| u.id),
            "text": text,
            "message_type": message_type,
            "telegram_message_id": msg.message_id,
        }),
    )
    .await;

    // --- Broadcast WebSocket ---
    state.hub.publish(
        "message",
        serde_json::json!({
            "bot_id": bot_id,
            "chat_id": chat.id,
            "from_user_id": from.map(|u| u.id),
            "text": text,
            "message_type": message_type,
        }),
    );

    // --- Konteks automation ---
    let context = serde_json::json!({
        "text": text,
        "chat_id": chat.id,
        "chat_type": chat.chat_type,
        "user_id": from.map(|u| u.id),
        "username": from.and_then(|u| u.username.clone()),
        "first_name": from.map(|u| u.first_name.clone()),
        "message_id": msg.message_id,
    });

    // --- User baru ---
    if is_new_user {
        if let Some(user) = from {
            log_activity(
                state,
                Some(bot_id),
                "new_user",
                Some("bot_user"),
                Some(&user.id.to_string()),
                serde_json::json!({"username": user.username, "first_name": user.first_name}),
                "info",
            )
            .await;
        }
        run_workflows(state, bot_id, "new_user", context.clone()).await;
    }

    // --- User bergabung ke grup (welcome message) ---
    if let Some(members) = &msg.new_chat_members {
        for member in members {
            if member.is_bot {
                continue;
            }
            log_activity(
                state,
                Some(bot_id),
                "user_joined_group",
                Some("bot_chat"),
                Some(&chat.id.to_string()),
                serde_json::json!({"user_id": member.id, "first_name": member.first_name}),
                "info",
            )
            .await;
            let join_ctx = serde_json::json!({
                "text": text,
                "chat_id": chat.id,
                "chat_type": chat.chat_type,
                "user_id": member.id,
                "username": member.username,
                "first_name": member.first_name,
                "message_id": msg.message_id,
            });
            run_workflows(state, bot_id, "user_joined_group", join_ctx).await;
            send_welcome_if_configured(state, bot_id, chat.id, member.first_name.as_str()).await;
        }
    }

    // --- Moderasi grup dasar ---
    if chat.chat_type == "group" || chat.chat_type == "supergroup" {
        moderate_group_message(state, bot_id, msg, &text).await;
    }

    // --- Command kustom ---
    if text.starts_with('/') {
        handle_command(state, bot_id, msg, &text).await;
        run_workflows(state, bot_id, "command", context.clone()).await;
    }

    // --- Trigger umum ---
    run_workflows(state, bot_id, "message_received", context.clone()).await;
    run_workflows(state, bot_id, "keyword", context).await;
}

async fn send_welcome_if_configured(state: &AppState, bot_id: &str, chat_id: i64, first_name: &str) {
    let chat_row = sqlx::query_as::<_, BotChat>(
        "SELECT * FROM bot_chats WHERE bot_id = ? AND chat_id = ?",
    )
    .bind(bot_id)
    .bind(chat_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let Some(chat_row) = chat_row else { return };
    let settings: serde_json::Value =
        serde_json::from_str(&chat_row.settings).unwrap_or(serde_json::json!({}));
    let Some(welcome) = settings.get("welcome_message").and_then(|v| v.as_str()) else {
        return;
    };
    if welcome.trim().is_empty() {
        return;
    }
    let text = render_template(
        welcome,
        &serde_json::json!({"first_name": first_name, "chat_id": chat_id}),
    );
    if let Ok(token) = get_bot_token(state, bot_id).await {
        let _ = state
            .telegram
            .send_message(&token, &SendMessageParams {
                chat_id,
                text,
                ..Default::default()
            })
            .await;
    }
}

/// Moderasi grup dasar: keyword filter, link filter, flood detection.
/// Eksekusi (hapus pesan) HANYA bila bot admin; bila bukan admin, hanya catat log.
async fn moderate_group_message(
    state: &AppState,
    bot_id: &str,
    msg: &TelegramMessage,
    text: &str,
) {
    let chat_id = msg.chat.id;
    let user_id = match msg.from.as_ref() {
        Some(u) => u.id,
        None => return,
    };
    let chat_row = sqlx::query_as::<_, BotChat>(
        "SELECT * FROM bot_chats WHERE bot_id = ? AND chat_id = ?",
    )
    .bind(bot_id)
    .bind(chat_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let Some(chat_row) = chat_row else { return };
    let settings: serde_json::Value =
        serde_json::from_str(&chat_row.settings).unwrap_or(serde_json::json!({}));
    let is_admin = chat_row.is_bot_admin == 1;

    let mut violations: Vec<String> = Vec::new();

    // Keyword filter.
    if settings
        .get("keyword_filter")
        .and_then(|k| k.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let keywords: Vec<String> = settings
            .get("keyword_filter")
            .and_then(|k| k.get("keywords"))
            .and_then(|k| k.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();
        let lower = text.to_lowercase();
        if keywords.iter().any(|kw| !kw.is_empty() && lower.contains(kw)) {
            violations.push("keyword_filter".to_string());
        }
    }

    // Link filter.
    if settings
        .get("link_filter")
        .and_then(|k| k.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let lower = text.to_lowercase();
        if lower.contains("http://")
            || lower.contains("https://")
            || lower.contains("www.")
            || lower.contains("t.me/")
        {
            violations.push("link_filter".to_string());
        }
    }

    // Flood / anti-spam: hitung pesan user dalam 60 detik terakhir.
    let flood_max = settings
        .get("anti_spam")
        .and_then(|a| a.get("max_per_minute"))
        .and_then(|v| v.as_u64())
        .or_else(|| {
            settings
                .get("flood")
                .and_then(|a| a.get("max_per_minute"))
                .and_then(|v| v.as_u64())
        })
        .unwrap_or(0) as usize;
    let anti_spam_on = settings
        .get("anti_spam")
        .and_then(|a| a.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || settings
            .get("flood")
            .and_then(|a| a.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
    if anti_spam_on && flood_max > 0 {
        let count = state.flood.record(bot_id, chat_id, user_id);
        if count > flood_max {
            violations.push("flood".to_string());
        }
    }

    if violations.is_empty() {
        return;
    }

    let action_taken = if is_admin {
        // Bot admin: hapus pesan yang melanggar.
        match get_bot_token(state, bot_id).await {
            Ok(token) => match state
                .telegram
                .delete_message(&token, chat_id, msg.message_id)
                .await
            {
                Ok(_) => "message_deleted",
                Err(e) => {
                    tracing::warn!("Gagal menghapus pesan melanggar di chat {chat_id}: {e}");
                    "delete_failed"
                }
            },
            Err(_) => "delete_failed",
        }
    } else {
        // Bukan admin: HANYA catat, jangan eksekusi moderasi.
        "logged_only_not_admin"
    };

    log_activity(
        state,
        Some(bot_id),
        "moderation",
        Some("moderation"),
        Some(&chat_id.to_string()),
        serde_json::json!({
            "chat_id": chat_id,
            "user_id": user_id,
            "message_id": msg.message_id,
            "violations": violations,
            "action": action_taken,
        }),
        "warning",
    )
    .await;
}

/// Command kustom: cari command aktif, naikkan usage_count, kirim
/// response_template sebagai auto-reply.
async fn handle_command(state: &AppState, bot_id: &str, msg: &TelegramMessage, text: &str) {
    let first_word = text.split_whitespace().next().unwrap_or("");
    let mut name = first_word.trim_start_matches('/').to_string();
    // Potong suffix @botusername (mis. /start@mybot).
    if let Some(at) = name.find('@') {
        name.truncate(at);
    }
    if name.is_empty() {
        return;
    }
    let command = sqlx::query_as::<_, BotCommand>(
        "SELECT * FROM bot_commands WHERE bot_id = ? AND name = ? AND is_enabled = 1",
    )
    .bind(bot_id)
    .bind(&name)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let Some(command) = command else { return };

    let _ = sqlx::query(
        "UPDATE bot_commands SET usage_count = usage_count + 1, updated_at = ? WHERE id = ?",
    )
    .bind(now_rfc3339())
    .bind(&command.id)
    .execute(&state.pool)
    .await;

    if command.response_template.trim().is_empty() {
        return;
    }
    let context = serde_json::json!({
        "first_name": msg.from.as_ref().map(|u| u.first_name.clone()),
        "username": msg.from.as_ref().and_then(|u| u.username.clone()),
        "chat_id": msg.chat.id,
        "text": text,
    });
    let reply = render_template(&command.response_template, &context);
    if let Ok(token) = get_bot_token(state, bot_id).await {
        let _ = state
            .telegram
            .send_message(&token, &SendMessageParams {
                chat_id: msg.chat.id,
                text: reply,
                reply_to_message_id: Some(msg.message_id),
                ..Default::default()
            })
            .await;
    }
    log_activity(
        state,
        Some(bot_id),
        "command_executed",
        Some("bot_command"),
        Some(&command.id),
        serde_json::json!({"command": name, "chat_id": msg.chat.id}),
        "info",
    )
    .await;
}
