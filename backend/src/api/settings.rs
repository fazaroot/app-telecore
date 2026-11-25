//! Pengaturan aplikasi (key-value di `app_settings`).
//!
//! Kunci `admin_pin_*` dikelola endpoint /api/security dan tidak bisa
//! dibaca/diubah lewat endpoint ini.

use axum::{extract::State, Json};
use serde_json::json;

use crate::error::{AppError, AppResult, Envelope};
use crate::models::{now_rfc3339, AppSetting};
use crate::state::AppState;

fn is_protected_key(key: &str) -> bool {
    key.starts_with("admin_pin")
}

pub async fn get_settings(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let rows = sqlx::query_as::<_, AppSetting>("SELECT * FROM app_settings ORDER BY key")
        .fetch_all(&state.pool)
        .await?;
    let mut map = serde_json::Map::new();
    for row in rows {
        if is_protected_key(&row.key) {
            continue;
        }
        // Coba parse sebagai JSON; bila gagal, perlakukan sebagai string.
        let value: serde_json::Value =
            serde_json::from_str(&row.value).unwrap_or(serde_json::Value::String(row.value));
        map.insert(row.key, value);
    }
    Ok(Envelope::ok(serde_json::Value::Object(map)))
}

/// PUT /api/settings — terima SATU setting `{"key": "...", "value": "..."}`
/// ATAU map banyak setting `{"theme": "dark", "language": "id"}`.
pub async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let pairs: Vec<(String, String)> = if let Some(obj) = body.as_object() {
        if let (Some(key), Some(value)) = (
            obj.get("key").and_then(|v| v.as_str()),
            obj.get("value").and_then(|v| v.as_str()),
        ) {
            vec![(key.to_string(), value.to_string())]
        } else {
            obj.iter()
                .map(|(k, v)| {
                    let val = match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (k.clone(), val)
                })
                .collect()
        }
    } else {
        return Err(AppError::Validation(
            "body harus berupa objek JSON".to_string(),
        ));
    };
    if pairs.is_empty() {
        return Err(AppError::Validation("tidak ada setting yang diubah".to_string()));
    }
    let now = now_rfc3339();
    let mut updated: Vec<String> = Vec::new();
    for (key, value) in pairs {
        if key.trim().is_empty() {
            return Err(AppError::Validation("key setting tidak boleh kosong".to_string()));
        }
        if is_protected_key(&key) {
            return Err(AppError::Validation(format!(
                "key '{key}' dilindungi — kelola via /api/security"
            )));
        }
        sqlx::query(
            "INSERT INTO app_settings (key, value, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(&key)
        .bind(&value)
        .bind(&now)
        .execute(&state.pool)
        .await?;
        updated.push(key);
    }
    Ok(Envelope::ok(json!({"updated": updated})))
}
