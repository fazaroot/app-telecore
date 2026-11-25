//! AI providers (v1.2): CRUD + tes koneksi + pengaturan AI per bot.
//!
//! - API key bersifat write-only: diterima di POST/PUT, disimpan terenkripsi
//!   AES-256-GCM (helper yang sama dengan token bot), TIDAK PERNAH
//!   dikembalikan — respons hanya `api_key_last4`.
//! - Tes koneksi tidak mengirim chat apa pun, hanya list models.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

use crate::error::{AppError, AppResult, Envelope};
use crate::models::{new_id, now_rfc3339, AiProvider};
use crate::security::Crypto;
use crate::services::ai::{build_test_url, count_models};
use crate::state::AppState;

pub const PROVIDER_TYPES: &[&str] = &["openai_compatible", "anthropic", "gemini"];

#[derive(Debug, Serialize)]
pub struct AiProviderPublic {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: Option<String>,
    pub model: String,
    pub api_key_last4: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

impl From<AiProvider> for AiProviderPublic {
    fn from(p: AiProvider) -> Self {
        Self {
            id: p.id,
            name: p.name,
            provider_type: p.provider_type,
            base_url: p.base_url,
            model: p.model,
            api_key_last4: p.api_key_last4,
            enabled: p.enabled == 1,
            created_at: p.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UpsertProviderRequest {
    pub name: String,
    pub provider_type: String,
    pub base_url: Option<String>,
    pub model: String,
    /// Write-only. Bila kosong pada PUT: key lama dipertahankan.
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
}

fn validate_provider(req: &UpsertProviderRequest) -> AppResult<()> {
    if req.name.trim().is_empty() {
        return Err(AppError::Validation("name tidak boleh kosong".to_string()));
    }
    if !PROVIDER_TYPES.contains(&req.provider_type.as_str()) {
        return Err(AppError::Validation(format!(
            "provider_type harus salah satu dari: {}",
            PROVIDER_TYPES.join(", ")
        )));
    }
    if req.model.trim().is_empty() {
        return Err(AppError::Validation("model tidak boleh kosong".to_string()));
    }
    if req.provider_type == "openai_compatible"
        && req.base_url.as_deref().map(str::trim).unwrap_or("").is_empty()
    {
        return Err(AppError::Validation(
            "openai_compatible membutuhkan base_url".to_string(),
        ));
    }
    Ok(())
}

fn last4(key: &str) -> String {
    let tail: String = key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if key.chars().count() <= 4 {
        "****".to_string()
    } else {
        format!("****{tail}")
    }
}

async fn fetch_provider(state: &AppState, id: &str) -> AppResult<AiProvider> {
    sqlx::query_as::<_, AiProvider>("SELECT * FROM ai_providers WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ai_provider {id} tidak ditemukan")))
}

pub async fn list_providers(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<Vec<AiProviderPublic>>>> {
    let rows = sqlx::query_as::<_, AiProvider>(
        "SELECT * FROM ai_providers ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Envelope::ok(
        rows.into_iter().map(AiProviderPublic::from).collect(),
    ))
}

pub async fn create_provider(
    State(state): State<AppState>,
    Json(req): Json<UpsertProviderRequest>,
) -> AppResult<Json<Envelope<AiProviderPublic>>> {
    validate_provider(&req)?;
    let (encrypted, last4v) = match req.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
        Some(key) => (
            Some(
                state
                    .crypto
                    .encrypt(key.trim())
                    .map_err(|e| AppError::Internal(format!("gagal mengenkripsi api_key: {e}")))?,
            ),
            Some(last4(key.trim())),
        ),
        None => (None, None),
    };
    let now = now_rfc3339();
    let id = new_id();
    sqlx::query(
        "INSERT INTO ai_providers (id, name, provider_type, base_url, model, \
         api_key_encrypted, api_key_last4, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(req.name.trim())
    .bind(&req.provider_type)
    .bind(req.base_url.as_deref().map(str::trim))
    .bind(req.model.trim())
    .bind(encrypted)
    .bind(last4v)
    .bind(if req.enabled.unwrap_or(true) { 1 } else { 0 })
    .bind(&now)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(AiProviderPublic::from(
        fetch_provider(&state, &id).await?,
    )))
}

pub async fn get_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<AiProviderPublic>>> {
    Ok(Envelope::ok(AiProviderPublic::from(
        fetch_provider(&state, &id).await?,
    )))
}

pub async fn update_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpsertProviderRequest>,
) -> AppResult<Json<Envelope<AiProviderPublic>>> {
    validate_provider(&req)?;
    let existing = fetch_provider(&state, &id).await?;
    let (encrypted, last4v) = match req.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
        Some(key) => (
            Some(
                state
                    .crypto
                    .encrypt(key.trim())
                    .map_err(|e| AppError::Internal(format!("gagal mengenkripsi api_key: {e}")))?,
            ),
            Some(last4(key.trim())),
        ),
        // Key kosong pada PUT: pertahankan key lama.
        None => (existing.api_key_encrypted.clone(), existing.api_key_last4.clone()),
    };
    sqlx::query(
        "UPDATE ai_providers SET name = ?, provider_type = ?, base_url = ?, model = ?, \
         api_key_encrypted = ?, api_key_last4 = ?, enabled = ? WHERE id = ?",
    )
    .bind(req.name.trim())
    .bind(&req.provider_type)
    .bind(req.base_url.as_deref().map(str::trim))
    .bind(req.model.trim())
    .bind(encrypted)
    .bind(last4v)
    .bind(if req.enabled.unwrap_or(existing.enabled == 1) {
        1
    } else {
        0
    })
    .bind(&id)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(AiProviderPublic::from(
        fetch_provider(&state, &id).await?,
    )))
}

pub async fn delete_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    fetch_provider(&state, &id).await?;
    // Bot yang memakai provider ini kembali ke AI nonaktif (provider_id
    // di-null-kan) agar tidak ada referensi menggantung.
    let _ = sqlx::query(
        "UPDATE bot_configurations SET ai_provider_id = NULL, ai_enabled = 0 WHERE ai_provider_id = ?",
    )
    .bind(&id)
    .execute(&state.pool)
    .await;
    sqlx::query("DELETE FROM ai_providers WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({"deleted": true, "id": id})))
}

pub async fn test_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let provider = fetch_provider(&state, &id).await?;
    let api_key = match &provider.api_key_encrypted {
        Some(enc) => Some(
            state
                .crypto
                .decrypt(enc)
                .map_err(|e| AppError::Internal(format!("gagal mendekripsi api_key: {e}")))?,
        ),
        None => None,
    };
    let url = build_test_url(&provider.provider_type, provider.base_url.as_deref())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Internal(format!("gagal membuat HTTP client: {e}")))?;
    let mut req = client.get(&url);
    match provider.provider_type.as_str() {
        "openai_compatible" => {
            if let Some(key) = &api_key {
                req = req.bearer_auth(key);
            }
        }
        "anthropic" => {
            let Some(key) = &api_key else {
                return Err(AppError::Validation(
                    "provider anthropic membutuhkan api_key".to_string(),
                ));
            };
            req = req
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01");
        }
        "gemini" => {
            let Some(key) = &api_key else {
                return Err(AppError::Validation(
                    "provider gemini membutuhkan api_key".to_string(),
                ));
            };
            req = req.query(&[("key", key)]);
        }
        _ => {}
    }
    let resp = req
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("tes koneksi gagal: {e}")))?;
    let status = resp.status();
    let json: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
    if !status.is_success() {
        return Ok(Envelope::ok(json!({
            "ok": false,
            "message": format!("provider mengembalikan HTTP {status}"),
        })));
    }
    Ok(Envelope::ok(json!({
        "ok": true,
        "message": "koneksi ke provider berhasil",
        "models_count": count_models(&provider.provider_type, &json),
    })))
}

// ---------------------------------------------------------------------------
// Pengaturan AI per bot: PUT /api/bots/{id}/ai
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UpdateBotAiRequest {
    pub ai_provider_id: Option<String>,
    pub ai_enabled: Option<bool>,
    pub ai_allow_groups: Option<bool>,
    pub ai_system_prompt: Option<String>,
    pub ai_max_tokens: Option<i64>,
    pub ai_temperature: Option<f64>,
}

pub async fn update_bot_ai(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateBotAiRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    // Bot harus ada.
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM bots WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(format!("bot {id} tidak ditemukan")));
    }
    if let Some(pid) = req.ai_provider_id.as_deref() {
        if !pid.trim().is_empty() {
            fetch_provider(&state, pid).await?;
        }
    }
    if let Some(m) = req.ai_max_tokens {
        if !(1..=8000).contains(&m) {
            return Err(AppError::Validation(
                "ai_max_tokens harus 1..8000".to_string(),
            ));
        }
    }
    if let Some(t) = req.ai_temperature {
        if !(0.0..=2.0).contains(&t) {
            return Err(AppError::Validation(
                "ai_temperature harus 0.0..2.0".to_string(),
            ));
        }
    }
    // Pastikan baris konfigurasi ada.
    let _ = crate::bot::get_or_create_config(&state, &id).await?;
    let now = now_rfc3339();
    // Update parsial: hanya field yang dikirim.
    let mut sets: Vec<String> = Vec::new();
    let mut binds: Vec<serde_json::Value> = Vec::new();
    if let Some(pid) = &req.ai_provider_id {
        sets.push("ai_provider_id = ?".to_string());
        binds.push(if pid.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(pid.clone())
        });
    }
    if let Some(v) = req.ai_enabled {
        sets.push("ai_enabled = ?".to_string());
        binds.push(serde_json::json!(if v { 1 } else { 0 }));
    }
    if let Some(v) = req.ai_allow_groups {
        sets.push("ai_allow_groups = ?".to_string());
        binds.push(serde_json::json!(if v { 1 } else { 0 }));
    }
    if let Some(v) = &req.ai_system_prompt {
        sets.push("ai_system_prompt = ?".to_string());
        binds.push(if v.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(v.clone())
        });
    }
    if let Some(v) = req.ai_max_tokens {
        sets.push("ai_max_tokens = ?".to_string());
        binds.push(serde_json::json!(v));
    }
    if let Some(v) = req.ai_temperature {
        sets.push("ai_temperature = ?".to_string());
        binds.push(serde_json::json!(v));
    }
    if sets.is_empty() {
        return Err(AppError::Validation("tidak ada field AI yang diubah".to_string()));
    }
    sets.push("updated_at = ?".to_string());
    binds.push(serde_json::Value::String(now.clone()));
    // Bangun query secara manual karena jumlah bind dinamis.
    let mut sql = format!(
        "UPDATE bot_configurations SET {} WHERE bot_id = ?",
        sets.join(", ")
    );
    let _ = &mut sql;
    let mut q = sqlx::query(&sql);
    for b in &binds {
        q = match b {
            serde_json::Value::Null => q.bind(Option::<String>::None),
            serde_json::Value::String(s) => q.bind(s.clone()),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    q.bind(i)
                } else if let Some(f) = n.as_f64() {
                    q.bind(f)
                } else {
                    q.bind(n.to_string())
                }
            }
            _ => q.bind(b.to_string()),
        };
    }
    q = q.bind(&id);
    q.execute(&state.pool).await?;
    let cfg = crate::bot::get_or_create_config(&state, &id).await?;
    Ok(Envelope::ok(json!({
        "bot_id": id,
        "ai_provider_id": cfg.ai_provider_id,
        "ai_enabled": cfg.ai_enabled == 1,
        "ai_allow_groups": cfg.ai_allow_groups == 1,
        "ai_system_prompt": cfg.ai_system_prompt,
        "ai_max_tokens": cfg.ai_max_tokens,
        "ai_temperature": cfg.ai_temperature,
    })))
}

/// Masking key untuk tampilan (hanya 4 karakter terakhir) — dipakai juga
/// untuk `api_key_last4` saat create/update.
#[allow(dead_code)]
pub fn mask_api_key(key: &str) -> String {
    Crypto::mask_token(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last4_shows_only_tail() {
        assert_eq!(last4("sk-abcdef123456"), "****3456");
        assert_eq!(last4("abc"), "****");
    }

    #[test]
    fn validate_rejects_bad_provider_type_and_missing_base_url() {
        let req = UpsertProviderRequest {
            name: "x".to_string(),
            provider_type: "openai".to_string(),
            base_url: None,
            model: "m".to_string(),
            api_key: None,
            enabled: None,
        };
        assert!(validate_provider(&req).is_err());
        let req = UpsertProviderRequest {
            name: "x".to_string(),
            provider_type: "openai_compatible".to_string(),
            base_url: None,
            model: "m".to_string(),
            api_key: None,
            enabled: None,
        };
        assert!(validate_provider(&req).is_err());
        let req = UpsertProviderRequest {
            name: "x".to_string(),
            provider_type: "gemini".to_string(),
            base_url: None,
            model: "gemini-2.0-flash".to_string(),
            api_key: None,
            enabled: None,
        };
        assert!(validate_provider(&req).is_ok());
    }

    #[test]
    fn public_view_never_contains_api_key() {
        let p = AiProvider {
            id: "p1".to_string(),
            name: "Test".to_string(),
            provider_type: "gemini".to_string(),
            base_url: None,
            model: "gemini-2.0-flash".to_string(),
            api_key_encrypted: Some("ENCRYPTED-SECRET".to_string()),
            api_key_last4: Some("****1234".to_string()),
            enabled: 1,
            created_at: "2026-11-25T00:00:00Z".to_string(),
        };
        let v = serde_json::to_value(AiProviderPublic::from(p)).unwrap();
        assert!(v.get("api_key_encrypted").is_none());
        assert!(v.get("api_key").is_none());
        assert_eq!(v["api_key_last4"], "****1234");
        assert_eq!(v["enabled"], true);
    }

    #[test]
    fn provider_api_key_encrypt_decrypt_round_trip() {
        // Enkripsi memakai helper yang sama dengan token bot (AES-256-GCM).
        let crypto = Crypto::from_key_bytes([9u8; 32]);
        let key = "sk-test-api-key-12345";
        let enc = crypto.encrypt(key).unwrap();
        assert_ne!(enc, key);
        assert_eq!(crypto.decrypt(&enc).unwrap(), key);
        assert_eq!(last4(key), "****2345");
    }
}
