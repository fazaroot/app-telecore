//! CRUD bot + connect/disconnect/test + config + webhook + export/import.
//!
//! Alur tambah bot: validasi format token -> enkripsi (AES-256-GCM) ->
//! simpan -> validasi via `getMe` ke api.telegram.org. Bila `getMe` gagal
//! (mis. token palsu), baris bot dihapus lagi dan dikembalikan error
//! `TELEGRAM_ERROR` yang rapi — bukan panic.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::bot::{get_bot_token, get_or_create_config};
use crate::error::{AppError, AppResult, Envelope};
use crate::models::{
    new_id, now_rfc3339, Bot, BotCommand, BotConfiguration, Paginated, Pagination,
};
use crate::security::Crypto;
use crate::services::log_activity;
use crate::services::outgoing_webhooks::dispatch_outgoing_event;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Validasi (pure, diuji unit)
// ---------------------------------------------------------------------------

pub fn validate_create_bot(name: &str, token: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::Validation("nama bot tidak boleh kosong".to_string()));
    }
    validate_token_format(token)
}

/// Validasi format token Telegram: <id-numerik>:<secret>. Sengaja longgar
/// di sini — validasi sebenarnya dilakukan via getMe ke api.telegram.org.
pub fn validate_token_format(token: &str) -> AppResult<()> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AppError::Validation("token bot tidak boleh kosong".to_string()));
    }
    let Some((id_part, secret)) = token.split_once(':') else {
        return Err(AppError::Validation(
            "format token tidak valid: harus <id>:<secret>".to_string(),
        ));
    };
    if id_part.is_empty() || !id_part.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::Validation(
            "format token tidak valid: bagian sebelum ':' harus angka".to_string(),
        ));
    }
    if secret.is_empty() {
        return Err(AppError::Validation(
            "format token tidak valid: bagian secret kosong".to_string(),
        ));
    }
    Ok(())
}

/// Pastikan token baru milik bot yang sama: telegram_id hasil getMe harus
/// sama dengan bot yang ada. Bot tanpa telegram_id (mis. hasil import
/// yang belum pernah terhubung) menerima identitas dari getMe.
pub fn ensure_token_identity(
    existing_telegram_id: Option<i64>,
    me: &crate::bot::telegram::TelegramUser,
) -> AppResult<()> {
    if let Some(existing) = existing_telegram_id {
        if existing != me.id {
            return Err(AppError::Conflict(
                "token baru milik bot Telegram lain: telegram_id token baru tidak sama dengan bot ini; ganti token hanya untuk bot yang sama".to_string(),
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// DTO
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateBotRequest {
    pub name: String,
    pub token: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBotRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_active: Option<bool>,
    /// Token baru (opsional). Kosong/whitespace = tidak mengganti token.
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub webhook_url: Option<String>,
    pub polling_enabled: Option<bool>,
    pub webhook_enabled: Option<bool>,
    pub auto_reply_enabled: Option<bool>,
    pub welcome_message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookRequest {
    /// "set" | "delete". Bila tidak diisi: "set" bila ada url, "delete" bila tidak.
    #[serde(default)]
    pub action: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListBotsQuery {
    pub status: Option<String>,
    #[serde(flatten)]
    pub pagination: Pagination,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn fetch_bot(state: &AppState, id: &str) -> AppResult<Bot> {
    sqlx::query_as::<_, Bot>("SELECT * FROM bots WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bot {id} tidak ditemukan")))
}

/// Terapkan mutual exclusion polling vs webhook setelah config berubah.
async fn apply_delivery_mode(
    state: &AppState,
    bot: &Bot,
    config: &BotConfiguration,
) -> AppResult<()> {
    if config.polling_enabled == 1 && config.webhook_enabled == 0 && bot.is_active == 1 {
        // Mode polling: pastikan webhook di Telegram dimatikan (best-effort),
        // lalu jalankan task polling.
        if let Ok(token) = get_bot_token(state, &bot.id).await {
            let _ = state.telegram.delete_webhook(&token, false).await;
        }
        state.polling.start(state.clone(), &bot.id).await;
    } else {
        state.polling.stop(&bot.id).await;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

pub async fn list_bots(
    State(state): State<AppState>,
    Query(q): Query<ListBotsQuery>,
) -> AppResult<Json<Envelope<Paginated<Bot>>>> {
    let (page, per_page) = q.pagination.normalized();
    let offset = q.pagination.offset();
    let (bots, total) = match &q.status {
        Some(status) => (
            sqlx::query_as::<_, Bot>(
                "SELECT * FROM bots WHERE status = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(status)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bots WHERE status = ?")
                .bind(status)
                .fetch_one(&state.pool)
                .await?,
        ),
        None => (
            sqlx::query_as::<_, Bot>(
                "SELECT * FROM bots ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bots")
                .fetch_one(&state.pool)
                .await?,
        ),
    };
    Ok(Envelope::ok(Paginated {
        items: bots,
        total,
        page,
        per_page,
    }))
}

pub async fn create_bot(
    State(state): State<AppState>,
    Json(req): Json<CreateBotRequest>,
) -> AppResult<Json<Envelope<Bot>>> {
    validate_create_bot(&req.name, &req.token)?;
    let token = req.token.trim().to_string();
    let encrypted = state
        .crypto
        .encrypt(&token)
        .map_err(|e| AppError::Internal(format!("gagal mengenkripsi token: {e}")))?;
    let masked = Crypto::mask_token(&token);
    let now = now_rfc3339();
    let id = new_id();

    sqlx::query(
        "INSERT INTO bots (id, name, token_encrypted, token_masked, status, description, created_at, updated_at) \
         VALUES (?, ?, ?, ?, 'disconnected', ?, ?, ?)",
    )
    .bind(&id)
    .bind(req.name.trim())
    .bind(&encrypted)
    .bind(&masked)
    .bind(req.description.as_deref())
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    let _ = get_or_create_config(&state, &id).await;

    // Validasi token ke Telegram. Bila gagal, hapus bot lagi (rollback manual)
    // dan kembalikan error yang rapi.
    match state.telegram.get_me(&token).await {
        Ok(me) => {
            sqlx::query(
                "UPDATE bots SET username = ?, telegram_id = ?, status = 'connected', updated_at = ? WHERE id = ?",
            )
            .bind(me.username.as_deref())
            .bind(me.id)
            .bind(now_rfc3339())
            .bind(&id)
            .execute(&state.pool)
            .await?;
        }
        Err(e) => {
            let _ = sqlx::query("DELETE FROM bots WHERE id = ?")
                .bind(&id)
                .execute(&state.pool)
                .await;
            return Err(AppError::Telegram(format!(
                "token tidak valid atau Telegram tidak dapat dihubungi: {}",
                crate::security::redact::redact_known_secret(&e.to_string(), &token)
            )));
        }
    }

    let bot = fetch_bot(&state, &id).await?;
    log_activity(
        &state,
        Some(&id),
        "bot_created",
        Some("bot"),
        Some(&id),
        json!({"name": bot.name}),
        "info",
    )
    .await;
    // Bot baru langsung polling bila config default mengizinkan.
    let config = get_or_create_config(&state, &id).await?;
    apply_delivery_mode(&state, &bot, &config).await?;
    Ok(Envelope::ok(bot))
}

pub async fn get_bot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<Bot>>> {
    Ok(Envelope::ok(fetch_bot(&state, &id).await?))
}

pub async fn update_bot(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateBotRequest>,
) -> AppResult<Json<Envelope<Bot>>> {
    let mut bot = fetch_bot(&state, &id).await?;
    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err(AppError::Validation("nama bot tidak boleh kosong".to_string()));
        }
        bot.name = name.trim().to_string();
    }
    if let Some(desc) = req.description {
        bot.description = Some(desc);
    }
    if let Some(active) = req.is_active {
        bot.is_active = if active { 1 } else { 0 };
    }

    // Ganti token (bila diisi): validasi format -> getMe -> pastikan
    // identitas bot sama -> enkripsi. Semua divalidasi SEBELUM DB diubah,
    // jadi token invalid/milik bot lain tidak mengubah apa pun.
    // Token tidak pernah muncul utuh di respons/log/error.
    let token_update = match req.token {
        Some(raw) if !raw.trim().is_empty() => {
            let token = raw.trim().to_string();
            validate_token_format(&token)?;
            let me = state.telegram.get_me(&token).await.map_err(|e| {
                AppError::Telegram(format!(
                    "token baru tidak valid atau Telegram tidak dapat dihubungi: {}",
                    crate::security::redact::redact_known_secret(&e.to_string(), &token)
                ))
            })?;
            ensure_token_identity(bot.telegram_id, &me)?;
            let encrypted = state
                .crypto
                .encrypt(&token)
                .map_err(|e| AppError::Internal(format!("gagal mengenkripsi token: {e}")))?;
            let masked = Crypto::mask_token(&token);
            Some((encrypted, masked, me))
        }
        _ => None,
    };

    bot.updated_at = now_rfc3339();
    if let Some((encrypted, masked, me)) = &token_update {
        bot.token_encrypted = encrypted.clone();
        bot.token_masked = masked.clone();
        bot.username = me.username.clone();
        bot.telegram_id = Some(me.id);
        bot.status = "connected".to_string();
        bot.consecutive_failures = 0;
        bot.last_check_at = Some(bot.updated_at.clone());
        sqlx::query(
            "UPDATE bots SET name = ?, description = ?, is_active = ?, token_encrypted = ?, \
             token_masked = ?, username = ?, telegram_id = ?, status = ?, \
             consecutive_failures = ?, last_check_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&bot.name)
        .bind(&bot.description)
        .bind(bot.is_active)
        .bind(&bot.token_encrypted)
        .bind(&bot.token_masked)
        .bind(&bot.username)
        .bind(bot.telegram_id)
        .bind(&bot.status)
        .bind(bot.consecutive_failures)
        .bind(&bot.last_check_at)
        .bind(&bot.updated_at)
        .bind(&bot.id)
        .execute(&state.pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE bots SET name = ?, description = ?, is_active = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&bot.name)
        .bind(&bot.description)
        .bind(bot.is_active)
        .bind(&bot.updated_at)
        .bind(&bot.id)
        .execute(&state.pool)
        .await?;
    }

    let bot = fetch_bot(&state, &id).await?;
    if token_update.is_some() {
        log_activity(
            &state,
            Some(&bot.id),
            "bot_token_updated",
            Some("bot"),
            Some(&bot.id),
            json!({"token_masked": bot.token_masked, "username": bot.username}),
            "info",
        )
        .await;
        // Restart polling bot ini dengan token baru. `apply_delivery_mode`
        // menghentikan task lama lalu menjalankan task baru bila mode
        // polling aktif; pada mode webhook/nonaktif polling dihentikan.
        let config = get_or_create_config(&state, &id).await?;
        apply_delivery_mode(&state, &bot, &config).await?;
    } else if bot.is_active == 0 {
        state.polling.stop(&bot.id).await;
    }
    Ok(Envelope::ok(bot))
}

pub async fn delete_bot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let bot = fetch_bot(&state, &id).await?;
    state.polling.stop(&id).await;
    sqlx::query("DELETE FROM bots WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    log_activity(
        &state,
        None,
        "bot_deleted",
        Some("bot"),
        Some(&id),
        json!({"name": bot.name}),
        "info",
    )
    .await;
    Ok(Envelope::ok(json!({"deleted": true, "id": id})))
}

// ---------------------------------------------------------------------------
// connect / disconnect / test
// ---------------------------------------------------------------------------

pub async fn connect_bot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<Bot>>> {
    fetch_bot(&state, &id).await?;
    let token = get_bot_token(&state, &id).await?;
    let me = state.telegram.get_me(&token).await?;
    sqlx::query(
        "UPDATE bots SET username = ?, telegram_id = ?, status = 'connected', is_active = 1, updated_at = ? WHERE id = ?",
    )
    .bind(me.username.as_deref())
    .bind(me.id)
    .bind(now_rfc3339())
    .bind(&id)
    .execute(&state.pool)
    .await?;
    let bot = fetch_bot(&state, &id).await?;
    let config = get_or_create_config(&state, &id).await?;
    apply_delivery_mode(&state, &bot, &config).await?;
    state.hub.publish(
        "bot_status",
        json!({"bot_id": id, "status": "connected"}),
    );
    Ok(Envelope::ok(bot))
}

pub async fn disconnect_bot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<Bot>>> {
    fetch_bot(&state, &id).await?;
    state.polling.stop(&id).await;
    sqlx::query("UPDATE bots SET status = 'disconnected', updated_at = ? WHERE id = ?")
        .bind(now_rfc3339())
        .bind(&id)
        .execute(&state.pool)
        .await?;
    let bot = fetch_bot(&state, &id).await?;
    state.hub.publish(
        "bot_status",
        json!({"bot_id": id, "status": "disconnected"}),
    );
    // --- Outgoing webhook: bot.disconnected (non-blocking, spawn) ---
    dispatch_outgoing_event(
        &state,
        "bot.disconnected",
        Some(&id),
        json!({"bot_name": bot.name}),
    )
    .await;
    Ok(Envelope::ok(bot))
}

pub async fn test_bot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    fetch_bot(&state, &id).await?;
    let token = get_bot_token(&state, &id).await?;
    match state.telegram.get_me(&token).await {
        Ok(me) => {
            sqlx::query(
                "UPDATE bots SET username = ?, telegram_id = ?, status = 'connected', updated_at = ? WHERE id = ?",
            )
            .bind(me.username.as_deref())
            .bind(me.id)
            .bind(now_rfc3339())
            .bind(&id)
            .execute(&state.pool)
            .await?;
            let bot = fetch_bot(&state, &id).await?;
            Ok(Envelope::ok(json!({"bot": bot, "telegram_user": me})))
        }
        Err(e) => {
            sqlx::query("UPDATE bots SET status = 'error', updated_at = ? WHERE id = ?")
                .bind(now_rfc3339())
                .bind(&id)
                .execute(&state.pool)
                .await?;
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Config + webhook
// ---------------------------------------------------------------------------

pub async fn get_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<BotConfiguration>>> {
    fetch_bot(&state, &id).await?;
    Ok(Envelope::ok(get_or_create_config(&state, &id).await?))
}

pub async fn update_config(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateConfigRequest>,
) -> AppResult<Json<Envelope<BotConfiguration>>> {
    let bot = fetch_bot(&state, &id).await?;
    let mut config = get_or_create_config(&state, &id).await?;

    if let Some(url) = req.webhook_url {
        config.webhook_url = if url.trim().is_empty() {
            None
        } else {
            Some(url.trim().to_string())
        };
    }
    if let Some(v) = req.auto_reply_enabled {
        config.auto_reply_enabled = if v { 1 } else { 0 };
    }
    if let Some(w) = req.welcome_message {
        config.welcome_message = if w.trim().is_empty() { None } else { Some(w) };
    }

    // Mutual exclusion polling <-> webhook.
    if req.polling_enabled == Some(true) {
        config.polling_enabled = 1;
        config.webhook_enabled = 0;
    } else if req.webhook_enabled == Some(true) {
        // Aktifkan webhook: set dulu ke Telegram (bila URL tersedia), baru simpan.
        if let Some(url) = &config.webhook_url {
            let token = get_bot_token(&state, &id).await?;
            state.telegram.set_webhook(&token, url).await?;
        }
        config.webhook_enabled = 1;
        config.polling_enabled = 0;
    } else {
        if let Some(v) = req.polling_enabled {
            config.polling_enabled = if v { 1 } else { 0 };
        }
        if let Some(v) = req.webhook_enabled {
            config.webhook_enabled = if v { 1 } else { 0 };
        }
    }
    config.updated_at = now_rfc3339();

    sqlx::query(
        "UPDATE bot_configurations SET webhook_url = ?, polling_enabled = ?, \
         webhook_enabled = ?, auto_reply_enabled = ?, welcome_message = ?, updated_at = ? \
         WHERE bot_id = ?",
    )
    .bind(&config.webhook_url)
    .bind(config.polling_enabled)
    .bind(config.webhook_enabled)
    .bind(config.auto_reply_enabled)
    .bind(&config.welcome_message)
    .bind(&config.updated_at)
    .bind(&id)
    .execute(&state.pool)
    .await?;

    apply_delivery_mode(&state, &bot, &config).await?;
    Ok(Envelope::ok(config))
}

pub async fn set_bot_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<WebhookRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    fetch_bot(&state, &id).await?;
    let mut config = get_or_create_config(&state, &id).await?;
    let token = get_bot_token(&state, &id).await?;

    // Frontend hanya mengirim {url}: simpulkan action bila tidak diisi.
    let action = req.action.clone().unwrap_or_else(|| {
        if req
            .url
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .is_some()
        {
            "set".to_string()
        } else {
            "delete".to_string()
        }
    });
    match action.as_str() {
        "set" => {
            let url = req
                .url
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .ok_or_else(|| {
                    AppError::Validation("url webhook wajib diisi untuk action 'set'".to_string())
                })?;
            if !url.starts_with("https://") {
                return Err(AppError::Validation(
                    "url webhook harus memakai https://".to_string(),
                ));
            }
            state.telegram.set_webhook(&token, url).await?;
            // Webhook aktif => hentikan polling bot ini.
            state.polling.stop(&id).await;
            config.webhook_url = Some(url.to_string());
            config.webhook_enabled = 1;
            config.polling_enabled = 0;
        }
        "delete" => {
            state.telegram.delete_webhook(&token, true).await?;
            config.webhook_url = None;
            config.webhook_enabled = 0;
        }
        other => {
            return Err(AppError::Validation(format!(
                "action tidak dikenal: '{other}' (gunakan 'set' atau 'delete')"
            )));
        }
    }
    config.updated_at = now_rfc3339();
    sqlx::query(
        "UPDATE bot_configurations SET webhook_url = ?, polling_enabled = ?, \
         webhook_enabled = ?, updated_at = ? WHERE bot_id = ?",
    )
    .bind(&config.webhook_url)
    .bind(config.polling_enabled)
    .bind(config.webhook_enabled)
    .bind(&config.updated_at)
    .bind(&id)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(json!({
        "bot_id": id,
        "action": action,
        "webhook_enabled": config.webhook_enabled == 1,
        "polling_enabled": config.polling_enabled == 1,
        "webhook_url": config.webhook_url,
    })))
}

// ---------------------------------------------------------------------------
// Export / import
// ---------------------------------------------------------------------------

/// Export bot: token diekspor dalam bentuk TERENKRIPSI (AES-256-GCM) —
/// TIDAK PERNAH plaintext. Import hanya berhasil bila server tujuan memakai
/// `TELECORE_ENCRYPTION_KEY` yang sama; bila kunci berbeda, import bot itu
/// dilewati dengan pesan error yang jelas (lihat `import_bots`).
pub async fn export_bots(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let bots = sqlx::query_as::<_, Bot>("SELECT * FROM bots ORDER BY created_at")
        .fetch_all(&state.pool)
        .await?;
    let mut out = Vec::new();
    for bot in bots {
        let config = sqlx::query_as::<_, BotConfiguration>(
            "SELECT * FROM bot_configurations WHERE bot_id = ?",
        )
        .bind(&bot.id)
        .fetch_optional(&state.pool)
        .await?;
        let commands = sqlx::query_as::<_, BotCommand>(
            "SELECT * FROM bot_commands WHERE bot_id = ?",
        )
        .bind(&bot.id)
        .fetch_all(&state.pool)
        .await?;
        out.push(json!({
            "name": bot.name,
            "username": bot.username,
            "description": bot.description,
            // Token TERENKRIPSI — bukan plaintext.
            "token_encrypted": bot.token_encrypted,
            "token_masked": bot.token_masked,
            "config": config,
            "commands": commands,
        }));
    }
    Ok(Envelope::ok(json!({
        "version": 1,
        "exported_at": now_rfc3339(),
        "note": "Token bot dalam file ini TERENKRIPSI (AES-256-GCM). Import hanya berhasil bila TELECORE_ENCRYPTION_KEY server tujuan sama.",
        "bots": out,
    })))
}

#[derive(Debug, Deserialize)]
pub struct ImportBot {
    pub name: String,
    pub token_encrypted: Option<String>,
    /// Alternatif: token plaintext (akan dienkripsi saat import).
    pub token: Option<String>,
    pub username: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub bots: Vec<ImportBot>,
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

pub async fn import_bots(
    State(state): State<AppState>,
    Json(req): Json<ImportRequest>,
) -> AppResult<Json<Envelope<ImportResult>>> {
    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for item in req.bots {
        if item.name.trim().is_empty() {
            skipped += 1;
            errors.push("Bot tanpa nama dilewati".to_string());
            continue;
        }
        // Tentukan token terenkripsi + masked.
        let (encrypted, masked): (String, String) = if let Some(enc) = item.token_encrypted {
            // Token terenkripsi dipakai langsung — TAPI verifikasi dulu bahwa
            // kunci enkripsi server ini bisa mendekripsinya.
            match state.crypto.decrypt(&enc) {
                Ok(plain) => (enc, Crypto::mask_token(&plain)),
                Err(_) => {
                    skipped += 1;
                    errors.push(format!(
                        "Bot '{}' dilewati: token terenkripsi tidak bisa didekripsi — \
                         TELECORE_ENCRYPTION_KEY server ini berbeda dengan server pengekspor",
                        item.name
                    ));
                    continue;
                }
            }
        } else if let Some(plain) = item.token {
            if let Err(e) = validate_create_bot(&item.name, &plain) {
                skipped += 1;
                errors.push(format!("Bot '{}' dilewati: {e}", item.name));
                continue;
            }
            match state.crypto.encrypt(plain.trim()) {
                Ok(enc) => (enc, Crypto::mask_token(plain.trim())),
                Err(e) => {
                    skipped += 1;
                    errors.push(format!("Bot '{}' dilewati: enkripsi gagal: {e}", item.name));
                    continue;
                }
            }
        } else {
            skipped += 1;
            errors.push(format!(
                "Bot '{}' dilewati: tidak ada token_encrypted maupun token",
                item.name
            ));
            continue;
        };

        let now = now_rfc3339();
        let id = new_id();
        let res = sqlx::query(
            "INSERT INTO bots (id, name, username, token_encrypted, token_masked, status, description, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, 'disconnected', ?, ?, ?)",
        )
        .bind(&id)
        .bind(item.name.trim())
        .bind(item.username.as_deref())
        .bind(&encrypted)
        .bind(&masked)
        .bind(item.description.as_deref())
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await;
        match res {
            Ok(_) => {
                let _ = get_or_create_config(&state, &id).await;
                imported += 1;
            }
            Err(e) => {
                skipped += 1;
                errors.push(format!("Bot '{}' gagal disimpan: {e}", item.name));
            }
        }
    }

    Ok(Envelope::ok(ImportResult {
        imported,
        skipped,
        errors,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_empty_token_and_name() {
        assert!(validate_create_bot("", "123456:ABC-DEF").is_err());
        assert!(validate_create_bot("BotKu", "").is_err());
        assert!(validate_create_bot("BotKu", "   ").is_err());
    }

    #[test]
    fn validation_rejects_bad_token_format() {
        assert!(validate_create_bot("BotKu", "tanpa-colon").is_err());
        assert!(validate_create_bot("BotKu", "abc:secret").is_err());
        assert!(validate_create_bot("BotKu", "123456:").is_err());
    }

    #[test]
    fn validation_accepts_fake_but_wellformed_token() {
        // Token palsu "123456:ABC-DEF" lolos validasi format — kegagalannya
        // harus ditangkap getMe (diuji di smoke test, bukan di sini).
        assert!(validate_create_bot("BotKu", "123456:ABC-DEF").is_ok());
    }

    #[test]
    fn token_identity_rejects_other_bot() {
        let me = crate::bot::telegram::TelegramUser {
            id: 222222222,
            is_bot: true,
            first_name: "Other".to_string(),
            last_name: None,
            username: Some("other_bot".to_string()),
            language_code: None,
        };
        assert!(ensure_token_identity(Some(111111111), &me).is_err());
        assert!(ensure_token_identity(Some(222222222), &me).is_ok());
        // Bot tanpa telegram_id menerima identitas getMe.
        assert!(ensure_token_identity(None, &me).is_ok());
    }

    fn fake_token(bot_id: i64, secret_word: &str) -> String {
        // Token sintetis yang jelas palsu, dibangun saat runtime.
        format!("{bot_id}:{}", secret_word.repeat(5))
    }

    async fn test_state_with_fake_telegram() -> AppState {
        use crate::bot::telegram::test_support::fake_telegram_server_with_handler;
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        // getMe palsu: id bot ditentukan dari awalan id pada token.
        let (base, _rx) = fake_telegram_server_with_handler(|req: &str| {
            if req.contains("/getMe") {
                if req.contains("222222222:") {
                    "{\"ok\":true,\"result\":{\"id\":222222222,\"is_bot\":true,\"first_name\":\"Other\",\"username\":\"other_bot\"}}".to_string()
                } else {
                    "{\"ok\":true,\"result\":{\"id\":111111111,\"is_bot\":true,\"first_name\":\"Test\",\"username\":\"new_bot\"}}".to_string()
                }
            } else {
                "{\"ok\":true,\"result\":true}".to_string()
            }
        });
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(":memory:")
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let config = crate::config::Config {
            host: "127.0.0.1".to_string(),
            port: 8080,
            database_url: "sqlite::memory:".to_string(),
            api_key: "test-key".to_string(),
            rust_log: "info".to_string(),
        };
        let crypto = std::sync::Arc::new(Crypto::from_key_bytes([9u8; 32]));
        let old_token = fake_token(111111111, "OLDFAKE");
        let encrypted = crypto.encrypt(&old_token).unwrap();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO bots (id, name, username, telegram_id, token_encrypted, token_masked, status, created_at, updated_at) \
             VALUES ('bot-1', 'Bot Test', 'old_bot', 111111111, ?, ?, 'connected', ?, ?)",
        )
        .bind(&encrypted)
        .bind(Crypto::mask_token(&old_token))
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        // Polling dimatikan agar test tidak menjalankan task polling.
        sqlx::query(
            "INSERT INTO bot_configurations (id, bot_id, polling_enabled, created_at, updated_at) \
             VALUES ('cfg-1', 'bot-1', 0, ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        let telegram = crate::bot::telegram::TelegramClient::with_base_url(base).unwrap();
        AppState::new(pool, config, crypto, telegram)
    }

    #[tokio::test]
    async fn update_bot_replaces_token_and_updates_masked_username() {
        let state = test_state_with_fake_telegram().await;
        let new_token = fake_token(111111111, "NEWFAKE");
        let Json(envelope) = update_bot(
            State(state.clone()),
            Path("bot-1".to_string()),
            Json(UpdateBotRequest {
                name: None,
                description: None,
                is_active: None,
                token: Some(new_token.clone()),
            }),
        )
        .await
        .unwrap();
        let bot = envelope.data.unwrap();
        assert_eq!(bot.token_masked, Crypto::mask_token(&new_token));
        assert_eq!(bot.username.as_deref(), Some("new_bot"));
        assert_eq!(bot.telegram_id, Some(111111111));
        // Token utuh tidak pernah muncul di respons.
        let serialized = serde_json::to_string(&bot).unwrap();
        assert!(!serialized.contains(&new_token));
        // Token baru tersimpan terenkripsi dan bisa didekripsi kembali.
        let stored = fetch_bot(&state, "bot-1").await.unwrap();
        assert_eq!(state.crypto.decrypt(&stored.token_encrypted).unwrap(), new_token);
    }

    #[tokio::test]
    async fn update_bot_rejects_token_of_other_bot() {
        let state = test_state_with_fake_telegram().await;
        let other_token = fake_token(222222222, "OTHERFAKE");
        let err = update_bot(
            State(state.clone()),
            Path("bot-1".to_string()),
            Json(UpdateBotRequest {
                name: None,
                description: None,
                is_active: None,
                token: Some(other_token.clone()),
            }),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("bot Telegram lain"));
        assert!(!err.to_string().contains(&other_token));
        // DB tidak berubah.
        let stored = fetch_bot(&state, "bot-1").await.unwrap();
        assert_eq!(stored.username.as_deref(), Some("old_bot"));
        assert_eq!(
            stored.token_masked,
            Crypto::mask_token(&fake_token(111111111, "OLDFAKE"))
        );
    }
}
