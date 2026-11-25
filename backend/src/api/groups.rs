//! Grup: list, detail, settings (legacy JSON blob + tabel v1.3),
//! moderation log, aksi moderasi, statistik.
//!
//! Aksi moderasi HANYA dijalankan bila bot adalah admin/creator di grup
//! (dicek via `getChatMember`); bila bukan admin, dikembalikan error 403
//! yang jelas. Setiap aksi moderasi (API maupun otomatis) dicatat ke
//! `group_moderation_log` DAN `activity_logs`.
//!
//! Settings v1.3 (`group_settings`) adalah sumber utama penegakan
//! moderasi; blob JSON lama di `bot_chats.settings` tetap dipertahankan
//! untuk kompatibilitas (PUT /api/groups/{id} merge JSON).

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::bot::get_bot_token;
use crate::error::{AppError, AppResult, Envelope};
use crate::models::{
    now_rfc3339, Bot, BotChat, GroupModerationLog, GroupSettings, Paginated, Pagination,
};
use crate::services::group_moderation::record_moderation;
use crate::services::segments::is_valid_violation_action;
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
    /// Durasi mute (menit). Bila tidak diisi: pakai
    /// group_settings.mute_duration_minutes, fallback 60.
    pub duration_minutes: Option<i64>,
    pub reason: Option<String>,
}

async fn fetch_chat(state: &AppState, id: &str) -> AppResult<BotChat> {
    sqlx::query_as::<_, BotChat>("SELECT * FROM bot_chats WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("grup {id} tidak ditemukan")))
}

/// Ambil baris group_settings; buat baris default bila belum ada.
pub async fn get_or_create_group_settings(
    state: &AppState,
    chat_id: i64,
) -> AppResult<GroupSettings> {
    if let Some(row) = sqlx::query_as::<_, GroupSettings>(
        "SELECT * FROM group_settings WHERE chat_id = ?",
    )
    .bind(chat_id)
    .fetch_optional(&state.pool)
    .await?
    {
        return Ok(row);
    }
    sqlx::query("INSERT OR IGNORE INTO group_settings (chat_id) VALUES (?)")
        .bind(chat_id)
        .execute(&state.pool)
        .await?;
    sqlx::query_as::<_, GroupSettings>("SELECT * FROM group_settings WHERE chat_id = ?")
        .bind(chat_id)
        .fetch_one(&state.pool)
        .await
        .map_err(AppError::from)
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

/// Legacy: merge JSON blob ke `bot_chats.settings` (PUT /api/groups/{id}).
/// Tetap berfungsi; settings v1.3 diatur lewat /settings.
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

// ---------------------------------------------------------------------------
// Group settings v1.3 (tabel group_settings)
// ---------------------------------------------------------------------------

pub async fn get_group_settings(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<GroupSettings>>> {
    let chat = fetch_chat(&state, &id).await?;
    Ok(Envelope::ok(
        get_or_create_group_settings(&state, chat.chat_id).await?,
    ))
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateGroupSettingsV13Request {
    pub welcome_enabled: Option<bool>,
    pub welcome_message: Option<String>,
    pub anti_spam_enabled: Option<bool>,
    pub flood_limit: Option<i64>,
    pub flood_window_seconds: Option<i64>,
    pub block_links: Option<bool>,
    pub banned_words: Option<Vec<String>>,
    pub violation_action: Option<String>,
    pub mute_duration_minutes: Option<i64>,
}

/// PUT /api/groups/{id}/settings — partial update diizinkan; field yang
/// tidak dikirim dipertahankan.
pub async fn update_group_settings_v13(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateGroupSettingsV13Request>,
) -> AppResult<Json<Envelope<GroupSettings>>> {
    let chat = fetch_chat(&state, &id).await?;
    let mut s = get_or_create_group_settings(&state, chat.chat_id).await?;
    if let Some(v) = req.welcome_enabled {
        s.welcome_enabled = if v { 1 } else { 0 };
    }
    if let Some(v) = req.welcome_message {
        s.welcome_message = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = req.anti_spam_enabled {
        s.anti_spam_enabled = if v { 1 } else { 0 };
    }
    if let Some(v) = req.flood_limit {
        if v < 1 {
            return Err(AppError::Validation(
                "flood_limit harus >= 1".to_string(),
            ));
        }
        s.flood_limit = v;
    }
    if let Some(v) = req.flood_window_seconds {
        if v < 1 {
            return Err(AppError::Validation(
                "flood_window_seconds harus >= 1".to_string(),
            ));
        }
        s.flood_window_seconds = v;
    }
    if let Some(v) = req.block_links {
        s.block_links = if v { 1 } else { 0 };
    }
    if let Some(words) = req.banned_words {
        if words.iter().any(|w| w.trim().is_empty()) {
            return Err(AppError::Validation(
                "banned_words tidak boleh berisi kata kosong".to_string(),
            ));
        }
        s.banned_words = serde_json::to_string(&words).unwrap();
    }
    if let Some(v) = req.violation_action {
        if !is_valid_violation_action(&v) {
            return Err(AppError::Validation(
                "violation_action harus delete, warn, atau mute".to_string(),
            ));
        }
        s.violation_action = v;
    }
    if let Some(v) = req.mute_duration_minutes {
        if v < 0 {
            return Err(AppError::Validation(
                "mute_duration_minutes tidak boleh negatif".to_string(),
            ));
        }
        s.mute_duration_minutes = v;
    }
    sqlx::query(
        "UPDATE group_settings SET welcome_enabled = ?, welcome_message = ?, \
         anti_spam_enabled = ?, flood_limit = ?, flood_window_seconds = ?, \
         block_links = ?, banned_words = ?, violation_action = ?, \
         mute_duration_minutes = ? WHERE chat_id = ?",
    )
    .bind(s.welcome_enabled)
    .bind(&s.welcome_message)
    .bind(s.anti_spam_enabled)
    .bind(s.flood_limit)
    .bind(s.flood_window_seconds)
    .bind(s.block_links)
    .bind(&s.banned_words)
    .bind(&s.violation_action)
    .bind(s.mute_duration_minutes)
    .bind(s.chat_id)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(s))
}

// ---------------------------------------------------------------------------
// Statistik grup
// ---------------------------------------------------------------------------

pub async fn group_stats(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let chat = fetch_chat(&state, &id).await?;
    let now = chrono::Utc::now();
    let today_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .to_rfc3339();
    let seven_days_ago = (now - chrono::Duration::days(7)).to_rfc3339();

    let messages_today = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM messages WHERE bot_id = ? AND chat_id = ? AND created_at >= ?",
    )
    .bind(&chat.bot_id)
    .bind(chat.chat_id)
    .bind(&today_start)
    .fetch_one(&state.pool)
    .await?;
    let messages_7d = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM messages WHERE bot_id = ? AND chat_id = ? AND created_at >= ?",
    )
    .bind(&chat.bot_id)
    .bind(chat.chat_id)
    .bind(&seven_days_ago)
    .fetch_one(&state.pool)
    .await?;
    let active_users_7d = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT from_user_id) FROM messages \
         WHERE bot_id = ? AND chat_id = ? AND created_at >= ? AND from_user_id IS NOT NULL",
    )
    .bind(&chat.bot_id)
    .bind(chat.chat_id)
    .bind(&seven_days_ago)
    .fetch_one(&state.pool)
    .await?;
    let top_rows = sqlx::query_as::<_, (Option<i64>, i64, Option<String>, Option<String>)>(
        "SELECT m.from_user_id, COUNT(*) AS cnt, u.username, u.first_name \
         FROM messages m LEFT JOIN bot_users u \
           ON u.bot_id = m.bot_id AND u.telegram_user_id = m.from_user_id \
         WHERE m.bot_id = ? AND m.chat_id = ? AND m.created_at >= ? \
           AND m.from_user_id IS NOT NULL \
         GROUP BY m.from_user_id ORDER BY cnt DESC LIMIT 5",
    )
    .bind(&chat.bot_id)
    .bind(chat.chat_id)
    .bind(&seven_days_ago)
    .fetch_all(&state.pool)
    .await?;
    let top_senders: Vec<serde_json::Value> = top_rows
        .into_iter()
        .map(|(user_id, count, username, first_name)| {
            json!({"user_id": user_id, "username": username, "first_name": first_name, "count": count})
        })
        .collect();

    // member_count live via getChatMemberCount; bila gagal (mis. bot bukan
    // admin / bukan anggota), pakai nilai tersimpan + tandai sumbernya.
    let (member_count, member_count_live) = match get_bot_token(&state, &chat.bot_id).await {
        Ok(token) => match state
            .telegram
            .get_chat_member_count(&token, chat.chat_id)
            .await
        {
            Ok(n) => {
                let _ = sqlx::query("UPDATE bot_chats SET member_count = ? WHERE id = ?")
                    .bind(n)
                    .bind(&chat.id)
                    .execute(&state.pool)
                    .await;
                (Some(n), true)
            }
            Err(_) => (chat.member_count, false),
        },
        Err(_) => (chat.member_count, false),
    };

    Ok(Envelope::ok(json!({
        "member_count": member_count,
        "member_count_live": member_count_live,
        "member_count_source": if member_count_live { "live" } else { "stored" },
        "messages_today": messages_today,
        "messages_7d": messages_7d,
        "active_users_7d": active_users_7d,
        "top_senders": top_senders,
    })))
}

// ---------------------------------------------------------------------------
// Log moderasi (tabel baru group_moderation_log)
// ---------------------------------------------------------------------------

pub async fn moderation_log(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> AppResult<Json<Envelope<Paginated<GroupModerationLog>>>> {
    let chat = fetch_chat(&state, &id).await?;
    let (page, per_page) = pagination.normalized();
    let offset = pagination.offset();
    let items = sqlx::query_as::<_, GroupModerationLog>(
        "SELECT * FROM group_moderation_log WHERE chat_id = ? \
         ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(chat.chat_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM group_moderation_log WHERE chat_id = ?",
    )
    .bind(chat.chat_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Envelope::ok(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}

/// Alias lama: GET /api/groups/{id}/moderation-logs — mengembalikan data
/// yang sama dari tabel baru `group_moderation_log`.
pub async fn moderation_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> AppResult<Json<Envelope<Paginated<GroupModerationLog>>>> {
    moderation_log(State(state), Path(id), Query(pagination)).await
}

// ---------------------------------------------------------------------------
// Moderasi
// ---------------------------------------------------------------------------

/// Aksi moderasi — hanya bila bot admin di grup.
pub async fn moderate(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ModerateRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let chat = fetch_chat(&state, &id).await?;
    if let Some(d) = req.duration_minutes {
        if d < 0 {
            return Err(AppError::Validation(
                "duration_minutes tidak boleh negatif".to_string(),
            ));
        }
    }
    let mut bot = sqlx::query_as::<_, Bot>("SELECT * FROM bots WHERE id = ?")
        .bind(&chat.bot_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bot {} tidak ditemukan", chat.bot_id)))?;
    let token = match get_bot_token(&state, &bot.id).await {
        Ok(t) => t,
        Err(e) => {
            record_moderation(
                &state,
                &chat.bot_id,
                chat.chat_id,
                req.user_id,
                &req.action,
                Some(&format!("GAGAL — token bot tidak bisa didekripsi: {e}")),
                json!({"chat_id": chat.chat_id, "user_id": req.user_id, "action": req.action, "via": "api", "result": "failed"}),
            )
            .await;
            return Err(e);
        }
    };

    // Butuh telegram_id bot sendiri untuk cek getChatMember; isi dari getMe
    // bila belum tersimpan.
    if bot.telegram_id.is_none() {
        match state.telegram.get_me(&token).await {
            Ok(me) => {
                bot.telegram_id = Some(me.id);
                let _ = sqlx::query("UPDATE bots SET telegram_id = ? WHERE id = ?")
                    .bind(me.id)
                    .bind(&bot.id)
                    .execute(&state.pool)
                    .await;
            }
            Err(e) => {
                // Telegram tidak terjangkau: catat percobaan, lalu kembalikan
                // error (tanpa crash). Aksi TIDAK dieksekusi.
                record_moderation(
                    &state,
                    &chat.bot_id,
                    chat.chat_id,
                    req.user_id,
                    &req.action,
                    Some(&format!("GAGAL — Telegram tidak terjangkau saat getMe: {e}")),
                    json!({"chat_id": chat.chat_id, "user_id": req.user_id, "action": req.action, "via": "api", "result": "failed"}),
                )
                .await;
                return Err(e);
            }
        }
    }
    let bot_tg_id = bot.telegram_id.unwrap();

    // Cek admin LANGSUNG ke Telegram (bukan hanya flag tersimpan), agar
    // "bukan admin" bisa dibedakan dari "Telegram tidak terjangkau".
    let member = match state
        .telegram
        .get_chat_member(&token, chat.chat_id, bot_tg_id)
        .await
    {
        Ok(m) => m,
        Err(e) => {
            record_moderation(
                &state,
                &chat.bot_id,
                chat.chat_id,
                req.user_id,
                &req.action,
                Some(&format!("GAGAL — Telegram tidak terjangkau saat cek admin: {e}")),
                json!({"chat_id": chat.chat_id, "user_id": req.user_id, "action": req.action, "via": "api", "result": "failed"}),
            )
            .await;
            return Err(e);
        }
    };
    let is_admin = member.status == "administrator" || member.status == "creator";
    let _ = sqlx::query("UPDATE bot_chats SET is_bot_admin = ? WHERE id = ?")
        .bind(if is_admin { 1 } else { 0 })
        .bind(&chat.id)
        .execute(&state.pool)
        .await;
    if !is_admin {
        record_moderation(
            &state,
            &chat.bot_id,
            chat.chat_id,
            req.user_id,
            &req.action,
            Some("DITOLAK — bot bukan admin di grup ini"),
            json!({"chat_id": chat.chat_id, "user_id": req.user_id, "action": req.action, "via": "api", "result": "denied_not_admin"}),
        )
        .await;
        return Err(AppError::Forbidden(
            "Bot bukan admin di grup ini — aksi moderasi tidak dapat dilakukan. \
             Jadikan bot sebagai admin grup terlebih dahulu."
                .to_string(),
        ));
    }

    // Durasi mute: request > group_settings > 60 menit.
    let settings = get_or_create_group_settings(&state, chat.chat_id).await?;
    let mute_minutes = req
        .duration_minutes
        .unwrap_or(settings.mute_duration_minutes)
        .max(0);

    let exec: AppResult<()> = async {
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
                let until = chrono::Utc::now().timestamp() + mute_minutes * 60;
                state
                    .telegram
                    .restrict_chat_member_until(&token, chat.chat_id, user_id, false, Some(until))
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
        Ok(())
    }
    .await;

    match exec {
        Ok(()) => {
            record_moderation(
                &state,
                &chat.bot_id,
                chat.chat_id,
                req.user_id,
                &req.action,
                req.reason.as_deref(),
                json!({
                    "chat_id": chat.chat_id,
                    "user_id": req.user_id,
                    "message_id": req.message_id,
                    "action": req.action,
                    "reason": req.reason,
                    "duration_minutes": if req.action == "mute" { Some(mute_minutes) } else { None },
                    "via": "api",
                }),
            )
            .await;
            Ok(Envelope::ok(json!({
                "success": true,
                "action": req.action,
                "chat_id": chat.chat_id,
                "duration_minutes": if req.action == "mute" { Some(mute_minutes) } else { None },
            })))
        }
        Err(e) => {
            // Validasi (400) vs kegagalan Telegram: keduanya dicatat agar
            // percobaan moderasi tidak hilang diam-diam.
            record_moderation(
                &state,
                &chat.bot_id,
                chat.chat_id,
                req.user_id,
                &req.action,
                Some(&format!("GAGAL — {e}")),
                json!({"chat_id": chat.chat_id, "user_id": req.user_id, "action": req.action, "via": "api", "result": "failed"}),
            )
            .await;
            Err(e)
        }
    }
}