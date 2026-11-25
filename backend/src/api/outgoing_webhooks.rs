//! Outgoing webhooks: CRUD + deliveries + test + retry.
//!
//! - Secret TIDAK PERNAH dikembalikan via API (kecuali sekali saat create,
//!   sebagai `secret` di respons). Respons list/detail hanya membawa
//!   `has_secret: bool`.
//! - Bila `secret` tidak disertakan saat create, server menggenerate satu
//!   (32 hex) dan menampilkannya sekali di respons create.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::error::{AppError, AppResult, Envelope};
use crate::models::{
    new_id, now_rfc3339, OutgoingWebhook, Paginated, Pagination, WebhookDelivery,
};
use crate::services::outgoing_webhooks::{
    deliver_to_webhook, generate_webhook_secret, redeliver, validate_events,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateWebhookRequest {
    pub name: String,
    pub url: String,
    pub events: Vec<String>,
    /// Secret opsional dari klien; bila kosong, digenerate server.
    pub secret: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWebhookRequest {
    pub name: Option<String>,
    pub url: Option<String>,
    pub events: Option<Vec<String>>,
    /// Secret baru (mengganti yang lama). Bila tidak diisi, secret lama tetap.
    pub secret: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListDeliveriesQuery {
    #[serde(flatten)]
    pub pagination: Pagination,
}

/// Representasi publik webhook: tanpa secret, hanya `has_secret`.
fn public_webhook(w: &OutgoingWebhook) -> serde_json::Value {
    json!({
        "id": w.id,
        "name": w.name,
        "url": w.url,
        "events": serde_json::from_str::<serde_json::Value>(&w.events).unwrap_or(json!([])),
        "enabled": w.enabled == 1,
        "has_secret": w.secret_encrypted.is_some(),
        "created_at": w.created_at,
        "updated_at": w.updated_at,
    })
}

async fn fetch_webhook(state: &AppState, id: &str) -> AppResult<OutgoingWebhook> {
    sqlx::query_as::<_, OutgoingWebhook>("SELECT * FROM outgoing_webhooks WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("webhook {id} tidak ditemukan")))
}

fn validate_url(url: &str) -> AppResult<()> {
    let url = url.trim();
    if url.is_empty() {
        return Err(AppError::Validation("url tidak boleh kosong".to_string()));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::Validation(
            "url webhook harus memakai http:// atau https://".to_string(),
        ));
    }
    Ok(())
}

pub async fn list_webhooks(
    State(state): State<AppState>,
    Query(p): Query<Pagination>,
) -> AppResult<Json<Envelope<Paginated<serde_json::Value>>>> {
    let (page, per_page) = p.normalized();
    let offset = p.offset();
    let items = sqlx::query_as::<_, OutgoingWebhook>(
        "SELECT * FROM outgoing_webhooks ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM outgoing_webhooks")
        .fetch_one(&state.pool)
        .await?;
    Ok(Envelope::ok(Paginated {
        items: items.iter().map(public_webhook).collect(),
        total,
        page,
        per_page,
    }))
}

pub async fn create_webhook(
    State(state): State<AppState>,
    Json(req): Json<CreateWebhookRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    if req.name.trim().is_empty() {
        return Err(AppError::Validation("name tidak boleh kosong".to_string()));
    }
    validate_url(&req.url)?;
    validate_events(&req.events)?;
    // Secret: pakai yang dikirim klien, atau generate baru. Hanya tampil
    // SEKALI di respons ini.
    let secret = req
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(generate_webhook_secret);
    let encrypted = state
        .crypto
        .encrypt(&secret)
        .map_err(|e| AppError::Internal(format!("gagal mengenkripsi secret: {e}")))?;
    let now = now_rfc3339();
    let id = new_id();
    sqlx::query(
        "INSERT INTO outgoing_webhooks (id, name, url, events, secret_encrypted, enabled, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(req.name.trim())
    .bind(req.url.trim())
    .bind(serde_json::to_string(&req.events).unwrap())
    .bind(&encrypted)
    .bind(if req.enabled.unwrap_or(true) { 1 } else { 0 })
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    let webhook = fetch_webhook(&state, &id).await?;
    let mut out = public_webhook(&webhook);
    out["secret"] = json!(secret);
    out["secret_note"] = json!("Secret hanya ditampilkan sekali di respons ini — simpan sekarang, karena tidak bisa diambil lagi.");
    Ok(Envelope::ok(out))
}

pub async fn get_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    Ok(Envelope::ok(public_webhook(&fetch_webhook(&state, &id).await?)))
}

pub async fn update_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWebhookRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let mut webhook = fetch_webhook(&state, &id).await?;
    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err(AppError::Validation("name tidak boleh kosong".to_string()));
        }
        webhook.name = name.trim().to_string();
    }
    if let Some(url) = req.url {
        validate_url(&url)?;
        webhook.url = url.trim().to_string();
    }
    if let Some(events) = req.events {
        validate_events(&events)?;
        webhook.events = serde_json::to_string(&events).unwrap();
    }
    // Secret baru (bila diisi) mengganti yang lama; respons membawa secret
    // baru sekali — sama seperti create.
    let mut new_secret: Option<String> = None;
    if let Some(secret) = req.secret.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let encrypted = state
            .crypto
            .encrypt(secret)
            .map_err(|e| AppError::Internal(format!("gagal mengenkripsi secret: {e}")))?;
        webhook.secret_encrypted = Some(encrypted);
        new_secret = Some(secret.to_string());
    }
    if let Some(enabled) = req.enabled {
        webhook.enabled = if enabled { 1 } else { 0 };
    }
    webhook.updated_at = now_rfc3339();
    sqlx::query(
        "UPDATE outgoing_webhooks SET name = ?, url = ?, events = ?, secret_encrypted = ?, enabled = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&webhook.name)
    .bind(&webhook.url)
    .bind(&webhook.events)
    .bind(&webhook.secret_encrypted)
    .bind(webhook.enabled)
    .bind(&webhook.updated_at)
    .bind(&webhook.id)
    .execute(&state.pool)
    .await?;
    let mut out = public_webhook(&fetch_webhook(&state, &id).await?);
    if let Some(secret) = new_secret {
        out["secret"] = json!(secret);
        out["secret_note"] = json!("Secret baru hanya ditampilkan sekali di respons ini.");
    }
    Ok(Envelope::ok(out))
}

pub async fn delete_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    fetch_webhook(&state, &id).await?;
    sqlx::query("DELETE FROM outgoing_webhooks WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({"deleted": true, "id": id})))
}

pub async fn list_deliveries(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(p): Query<ListDeliveriesQuery>,
) -> AppResult<Json<Envelope<Paginated<WebhookDelivery>>>> {
    fetch_webhook(&state, &id).await?;
    let (page, per_page) = p.pagination.normalized();
    let offset = p.pagination.offset();
    let items = sqlx::query_as::<_, WebhookDelivery>(
        "SELECT * FROM webhook_deliveries WHERE webhook_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(&id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM webhook_deliveries WHERE webhook_id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Envelope::ok(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}

/// Kirim event `test` sungguhan ke URL target dan catat delivery-nya.
pub async fn test_webhook(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<WebhookDelivery>>> {
    let webhook = fetch_webhook(&state, &id).await?;
    // "test" bukan event langganan — dikirim sebagai uji koneksi ke URL target.
    let payload = json!({
        "event": "test",
        "bot_id": null,
        "data": {"message": "TeleCore outgoing webhook test"},
        "sent_at": now_rfc3339(),
    });
    let delivery = deliver_to_webhook(&state, &webhook, "test", &payload).await?;
    Ok(Envelope::ok(delivery))
}

pub async fn retry_delivery(
    State(state): State<AppState>,
    Path((id, delivery_id)): Path<(String, String)>,
) -> AppResult<Json<Envelope<WebhookDelivery>>> {
    fetch_webhook(&state, &id).await?;
    let delivery = sqlx::query_as::<_, WebhookDelivery>(
        "SELECT * FROM webhook_deliveries WHERE id = ? AND webhook_id = ?",
    )
    .bind(&delivery_id)
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| {
        AppError::NotFound(format!("delivery {delivery_id} tidak ditemukan untuk webhook {id}"))
    })?;
    let _ = delivery;
    Ok(Envelope::ok(redeliver(&state, &delivery_id).await?))
}
