//! Webhook Telegram: POST /api/webhooks/telegram/{bot_id}
//!
//! TANPA auth API key (Telegram tidak mengirim header kustom kita).
//! Validasi: bot_id harus ada di DB. Update diproses async (spawn) agar
//! respons 200 cepat kembali ke Telegram; pemrosesan memakai jalur yang
//! sama dengan long polling (`services::process_update`).

use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::json;

use crate::bot::telegram::Update;
use crate::error::{AppError, AppResult, Envelope};
use crate::services::process_update;
use crate::state::AppState;

pub async fn telegram_webhook(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Json(update): Json<Update>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM bots WHERE id = ?")
        .bind(&bot_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(format!(
            "bot {bot_id} tidak ditemukan"
        )));
    }
    tokio::spawn(async move {
        process_update(&state, &bot_id, &update).await;
    });
    Ok(Envelope::ok(json!({"received": true})))
}
