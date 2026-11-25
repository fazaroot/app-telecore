//! Integrasi Telegram: client Bot API + long polling per bot.

pub mod polling;
pub mod telegram;

pub use polling::PollingManager;

use crate::error::{AppError, AppResult};
use crate::models::{Bot, BotConfiguration};
use crate::state::AppState;

/// Ambil token bot (dekripsi AES-256-GCM). Dipakai tepat sebelum memanggil
/// Telegram API; token tidak pernah disimpan di struct berumur panjang.
pub async fn get_bot_token(state: &AppState, bot_id: &str) -> AppResult<String> {
    let bot = sqlx::query_as::<_, Bot>("SELECT * FROM bots WHERE id = ?")
        .bind(bot_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("bot {bot_id} tidak ditemukan")))?;
    state
        .crypto
        .decrypt(&bot.token_encrypted)
        .map_err(|e| AppError::Internal(format!("gagal mendekripsi token bot: {e}")))
}

/// Ambil konfigurasi bot (buat baris default bila belum ada).
pub async fn get_or_create_config(
    state: &AppState,
    bot_id: &str,
) -> AppResult<BotConfiguration> {
    if let Some(cfg) = sqlx::query_as::<_, BotConfiguration>(
        "SELECT * FROM bot_configurations WHERE bot_id = ?",
    )
    .bind(bot_id)
    .fetch_optional(&state.pool)
    .await?
    {
        return Ok(cfg);
    }
    let now = crate::models::now_rfc3339();
    let id = crate::models::new_id();
    sqlx::query(
        "INSERT INTO bot_configurations (id, bot_id, created_at, updated_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(bot_id)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    let cfg = sqlx::query_as::<_, BotConfiguration>(
        "SELECT * FROM bot_configurations WHERE bot_id = ?",
    )
    .bind(bot_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(cfg)
}

/// Jalankan task polling untuk semua bot aktif dengan `polling_enabled = 1`
/// (dipanggil sekali saat server start).
pub async fn start_polling_for_active_bots(state: &AppState) {
    let bots = sqlx::query_as::<_, Bot>(
        "SELECT b.* FROM bots b \
         JOIN bot_configurations c ON c.bot_id = b.id \
         WHERE b.is_active = 1 AND c.polling_enabled = 1 AND c.webhook_enabled = 0",
    )
    .fetch_all(&state.pool)
    .await;
    match bots {
        Ok(list) => {
            for bot in list {
                tracing::info!("Memulai polling untuk bot {} ({})", bot.name, bot.id);
                state.polling.start(state.clone(), &bot.id).await;
            }
        }
        Err(e) => tracing::error!("Gagal memuat bot aktif untuk polling: {e}"),
    }
}
