//! Keamanan: status (tanpa nilai secret), PIN admin (SHA-256 + salt),
//! lock/unlock session, daftar session.
//!
//! Catatan: API key berasal dari env `TELECORE_API_KEY` dan TIDAK bisa diubah
//! via API. Rotasi dilakukan dengan mengubah env lalu restart server — ini
//! disengaja agar kunci tidak bisa dicuri lewat endpoint.

use axum::{extract::State, Json};
use serde::Deserialize;
use serde_json::json;

use crate::error::{AppError, AppResult, Envelope};
use crate::models::{now_rfc3339, AdminSession};
use crate::security::crypto::{generate_salt_hex, hash_pin};
use crate::state::AppState;

const ADMIN_SESSION_ID: &str = "admin";

#[derive(Debug, Deserialize)]
pub struct ChangePinRequest {
    pub current_pin: Option<String>,
    pub new_pin: String,
}

#[derive(Debug, Deserialize)]
pub struct UnlockRequest {
    pub pin: String,
}

async fn get_or_create_admin_session(state: &AppState) -> AppResult<AdminSession> {
    if let Some(s) = sqlx::query_as::<_, AdminSession>(
        "SELECT * FROM admin_sessions WHERE id = ?",
    )
    .bind(ADMIN_SESSION_ID)
    .fetch_optional(&state.pool)
    .await?
    {
        return Ok(s);
    }
    let now = now_rfc3339();
    sqlx::query(
        "INSERT INTO admin_sessions (id, created_at, updated_at) VALUES (?, ?, ?)",
    )
    .bind(ADMIN_SESSION_ID)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    let s = sqlx::query_as::<_, AdminSession>("SELECT * FROM admin_sessions WHERE id = ?")
        .bind(ADMIN_SESSION_ID)
        .fetch_one(&state.pool)
        .await?;
    Ok(s)
}

/// Status keamanan — TANPA nilai secret apa pun.
pub async fn status(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let session = get_or_create_admin_session(&state).await?;
    Ok(Envelope::ok(json!({
        "encryption_active": true,
        "encryption_algorithm": "AES-256-GCM",
        "api_key_configured": !state.config.api_key.is_empty(),
        "pin_configured": session.pin_hash.is_some(),
        "session_locked": session.is_locked == 1,
        "uptime_seconds": state.started_at.elapsed().as_secs(),
        "api_key_rotation": "API key berasal dari env TELECORE_API_KEY; rotasi dilakukan dengan mengubah env lalu restart server.",
    })))
}

pub async fn change_pin(
    State(state): State<AppState>,
    Json(req): Json<ChangePinRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    if req.new_pin.len() < 4 || req.new_pin.len() > 64 {
        return Err(AppError::Validation(
            "PIN baru harus 4–64 karakter".to_string(),
        ));
    }
    let session = get_or_create_admin_session(&state).await?;
    if let (Some(stored_hash), Some(salt)) = (&session.pin_hash, &session.salt) {
        // PIN sudah ada: wajib verifikasi PIN lama.
        let current = req.current_pin.as_deref().unwrap_or("");
        if hash_pin(current, salt) != *stored_hash {
            return Err(AppError::Unauthorized("PIN lama salah".to_string()));
        }
    }
    let salt = generate_salt_hex();
    let hash = hash_pin(&req.new_pin, &salt);
    sqlx::query(
        "UPDATE admin_sessions SET pin_hash = ?, salt = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&hash)
    .bind(&salt)
    .bind(now_rfc3339())
    .bind(ADMIN_SESSION_ID)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(json!({"pin_changed": true})))
}

pub async fn lock(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    get_or_create_admin_session(&state).await?;
    sqlx::query("UPDATE admin_sessions SET is_locked = 1, updated_at = ? WHERE id = ?")
        .bind(now_rfc3339())
        .bind(ADMIN_SESSION_ID)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({"locked": true})))
}

pub async fn unlock(
    State(state): State<AppState>,
    Json(req): Json<UnlockRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let session = get_or_create_admin_session(&state).await?;
    let (Some(stored_hash), Some(salt)) = (&session.pin_hash, &session.salt) else {
        return Err(AppError::Validation(
            "PIN belum diatur — atur dulu via /api/security/change-pin".to_string(),
        ));
    };
    if hash_pin(&req.pin, salt) != *stored_hash {
        return Err(AppError::Unauthorized("PIN salah".to_string()));
    }
    let now = now_rfc3339();
    sqlx::query(
        "UPDATE admin_sessions SET is_locked = 0, last_unlocked_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(ADMIN_SESSION_ID)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(json!({"unlocked": true})))
}

pub async fn list_sessions(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<Vec<AdminSession>>>> {
    // pin_hash & salt di-skip dari serialisasi (lihat models.rs).
    let sessions = sqlx::query_as::<_, AdminSession>(
        "SELECT * FROM admin_sessions ORDER BY created_at",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Envelope::ok(sessions))
}
