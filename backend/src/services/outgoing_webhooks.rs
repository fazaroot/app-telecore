//! Outgoing webhooks (v1.1).
//!
//! - Secret webhook TIDAK PERNAH disimpan mentah: disimpan terenkripsi
//!   AES-256-GCM di `outgoing_webhooks.secret_encrypted` (HMAC butuh secret
//!   mentah saat pengiriman, jadi hash saja tidak cukup).
//! - Pengiriman: `POST` JSON `{event, bot_id, data, sent_at}` dengan timeout
//!   10 detik, header `X-TeleCore-Event` + `X-TeleCore-Signature`
//!   (HMAC-SHA256 hex dari body mentah, bila webhook punya secret).
//! - Dispatch TIDAK PERNAH memblokir alur utama: tiap webhook dikirim lewat
//!   `tokio::spawn`; kegagalan hanya dicatat sebagai delivery gagal.

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;

use crate::error::{AppError, AppResult};
use crate::models::{new_id, now_rfc3339, OutgoingWebhook, WebhookDelivery};
use crate::state::AppState;

/// Event yang didukung outgoing webhook.
pub const SUPPORTED_EVENTS: &[&str] = &[
    "message.inbound",
    "automation.executed",
    "bot.disconnected",
    "scheduled_task.completed",
];

pub fn is_supported_event(event: &str) -> bool {
    SUPPORTED_EVENTS.contains(&event)
}

/// HMAC-SHA256 hex dari body mentah dengan secret. Pure — diuji unit.
pub fn sign_webhook_payload(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC menerima kunci sepanjang apa pun");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Verifikasi signature HMAC (perbandingan panjang-tetap). Pure — diuji unit.
pub fn verify_webhook_signature(secret: &str, body: &[u8], signature_hex: &str) -> bool {
    let expected = sign_webhook_payload(secret, body);
    if expected.len() != signature_hex.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.bytes().zip(signature_hex.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Generate secret webhook baru (32 karakter hex acak).
pub fn generate_webhook_secret() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Validasi daftar event: tidak kosong dan semuanya didukung.
pub fn validate_events(events: &[String]) -> AppResult<()> {
    if events.is_empty() {
        return Err(AppError::Validation(
            "events tidak boleh kosong".to_string(),
        ));
    }
    for e in events {
        if !is_supported_event(e) {
            return Err(AppError::Validation(format!(
                "event tidak didukung: '{e}' (yang didukung: {})",
                SUPPORTED_EVENTS.join(", ")
            )));
        }
    }
    Ok(())
}

/// Dekripsi secret webhook (None bila webhook tidak punya secret).
fn webhook_secret(state: &AppState, webhook: &OutgoingWebhook) -> Option<String> {
    webhook
        .secret_encrypted
        .as_ref()
        .and_then(|enc| state.crypto.decrypt(enc).ok())
}

/// Kirim SATU payload ke SATU webhook dan catat delivery-nya.
/// Mengembalikan baris delivery terbaru (status delivered/failed).
pub async fn deliver_to_webhook(
    state: &AppState,
    webhook: &OutgoingWebhook,
    event: &str,
    payload: &serde_json::Value,
) -> AppResult<WebhookDelivery> {
    let id = new_id();
    let now = now_rfc3339();
    let payload_str = payload.to_string();
    sqlx::query(
        "INSERT INTO webhook_deliveries (id, webhook_id, event, payload, status, attempts, created_at, updated_at) \
         VALUES (?, ?, ?, ?, 'pending', 0, ?, ?)",
    )
    .bind(&id)
    .bind(&webhook.id)
    .bind(event)
    .bind(&payload_str)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    let secret = webhook_secret(state, webhook);
    let result = post_webhook(&webhook.url, event, secret.as_deref(), payload_str.as_bytes()).await;

    let (status, http_status, last_error) = match &result {
        Ok(code) => ("delivered", Some(*code), None),
        Err(e) => ("failed", None, Some(e.clone())),
    };
    sqlx::query(
        "UPDATE webhook_deliveries SET status = ?, http_status = ?, attempts = 1, last_error = ?, updated_at = ? WHERE id = ?",
    )
    .bind(status)
    .bind(http_status)
    .bind(last_error)
    .bind(now_rfc3339())
    .bind(&id)
    .execute(&state.pool)
    .await?;

    let delivery = sqlx::query_as::<_, WebhookDelivery>(
        "SELECT * FROM webhook_deliveries WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;
    Ok(delivery)
}

/// POST mentah ke URL webhook. Ok(http_status) bila request sampai ke server
/// (status HTTP berapa pun); Err(pesan) bila koneksi/timeout gagal.
async fn post_webhook(
    url: &str,
    event: &str,
    secret: Option<&str>,
    body: &[u8],
) -> Result<i64, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-TeleCore-Event", event);
    if let Some(secret) = secret {
        let sig = sign_webhook_payload(secret, body);
        req = req.header("X-TeleCore-Signature", sig);
    }
    let resp = req
        .body(body.to_vec())
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(resp.status().as_u16() as i64)
}

/// Kirim ulang delivery yang sudah tersimpan (payload lama dipakai apa adanya).
pub async fn redeliver(
    state: &AppState,
    delivery_id: &str,
) -> AppResult<WebhookDelivery> {
    let delivery = sqlx::query_as::<_, WebhookDelivery>(
        "SELECT * FROM webhook_deliveries WHERE id = ?",
    )
    .bind(delivery_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("delivery {delivery_id} tidak ditemukan")))?;
    let webhook = sqlx::query_as::<_, OutgoingWebhook>(
        "SELECT * FROM outgoing_webhooks WHERE id = ?",
    )
    .bind(&delivery.webhook_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| {
        AppError::NotFound(format!(
            "webhook {} tidak ditemukan (mungkin sudah dihapus)",
            delivery.webhook_id
        ))
    })?;
    let secret = webhook_secret(state, &webhook);
    let result = post_webhook(
        &webhook.url,
        &delivery.event,
        secret.as_deref(),
        delivery.payload.as_bytes(),
    )
    .await;
    let (status, http_status, last_error) = match &result {
        Ok(code) => ("delivered", Some(*code), None),
        Err(e) => ("failed", None, Some(e.clone())),
    };
    sqlx::query(
        "UPDATE webhook_deliveries SET status = ?, http_status = ?, attempts = attempts + 1, last_error = ?, updated_at = ? WHERE id = ?",
    )
    .bind(status)
    .bind(http_status)
    .bind(last_error)
    .bind(now_rfc3339())
    .bind(&delivery.id)
    .execute(&state.pool)
    .await?;
    let updated = sqlx::query_as::<_, WebhookDelivery>(
        "SELECT * FROM webhook_deliveries WHERE id = ?",
    )
    .bind(&delivery.id)
    .fetch_one(&state.pool)
    .await?;
    Ok(updated)
}

/// Dispatch event ke semua webhook aktif yang berlangganan event tersebut.
/// Non-blocking: tiap pengiriman berjalan di `tokio::spawn` sendiri;
/// kegagalan hanya dicatat sebagai delivery gagal, tidak pernah melempar
/// error ke pemanggil.
pub async fn dispatch_outgoing_event(
    state: &AppState,
    event: &str,
    bot_id: Option<&str>,
    data: serde_json::Value,
) {
    if !is_supported_event(event) {
        tracing::warn!("dispatch_outgoing_event: event tidak didukung: {event}");
        return;
    }
    let webhooks = sqlx::query_as::<_, OutgoingWebhook>(
        "SELECT * FROM outgoing_webhooks WHERE enabled = 1",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let payload = serde_json::json!({
        "event": event,
        "bot_id": bot_id,
        "data": data,
        "sent_at": now_rfc3339(),
    });
    for webhook in webhooks {
        let subscribed: Vec<String> =
            serde_json::from_str(&webhook.events).unwrap_or_default();
        if !subscribed.iter().any(|e| e == event) {
            continue;
        }
        let state = state.clone();
        let event = event.to_string();
        let payload = payload.clone();
        tokio::spawn(async move {
            if let Err(e) = deliver_to_webhook(&state, &webhook, &event, &payload).await {
                tracing::warn!(
                    "Outgoing webhook {} gagal dicatat: {e}",
                    webhook.id
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_sign_and_verify_round_trip() {
        let secret = "webhook-secret-rahasia";
        let body = br#"{"event":"test","bot_id":null}"#;
        let sig = sign_webhook_payload(secret, body);
        assert_eq!(sig.len(), 64); // hex SHA-256
        assert!(verify_webhook_signature(secret, body, &sig));
        // Body berbeda => signature tidak cocok.
        assert!(!verify_webhook_signature(secret, b"{}", &sig));
        // Secret berbeda => signature tidak cocok.
        assert!(!verify_webhook_signature("secret-lain", body, &sig));
        // Signature rusak => tidak cocok.
        assert!(!verify_webhook_signature(secret, body, "00"));
        // Deterministik: dua kali sign menghasilkan nilai sama.
        assert_eq!(sign_webhook_payload(secret, body), sig);
    }

    #[test]
    fn event_validation() {
        assert!(is_supported_event("message.inbound"));
        assert!(is_supported_event("scheduled_task.completed"));
        assert!(!is_supported_event("message.deleted"));
        assert!(validate_events(&["message.inbound".to_string()]).is_ok());
        assert!(validate_events(&[]).is_err());
        assert!(validate_events(&["tidak-ada".to_string()]).is_err());
    }

    #[test]
    fn generated_webhook_secret_is_hex_32() {
        let s = generate_webhook_secret();
        assert_eq!(s.len(), 32);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(s, generate_webhook_secret());
    }
}
