//! Middleware: autentikasi API key, rate limit per IP, redaksi log.

use axum::{
    extract::{ConnectInfo, State},
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::AppError;
use crate::models::now_rfc3339;
use crate::services::tokens::hash_token;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Autentikasi API key (header X-API-Key)
// ---------------------------------------------------------------------------

/// Terima salah satu:
/// 1. API key env (`TELECORE_API_KEY`), atau
/// 2. Personal API token (`tc_...`) yang hash SHA-256-nya ada di `api_tokens`.
///    Bila token personal cocok, `last_used_at` diupdate.
/// Token personal tidak pernah dibandingkan dengan nilai mentah yang tersimpan
/// (yang tersimpan hanya hash).
pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let provided = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let expected = &state.config.api_key;
    if !expected.is_empty() && constant_time_eq(&provided, expected) {
        return next.run(req).await;
    }
    if provided.starts_with("tc_") {
        let hash = hash_token(&provided);
        let found: Option<(String,)> =
            sqlx::query_as("SELECT id FROM api_tokens WHERE token_hash = ?")
                .bind(&hash)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten();
        if let Some((token_id,)) = found {
            let _ = sqlx::query(
                "UPDATE api_tokens SET last_used_at = ? WHERE id = ?",
            )
            .bind(now_rfc3339())
            .bind(&token_id)
            .execute(&state.pool)
            .await;
            return next.run(req).await;
        }
    }
    if expected.is_empty() {
        return AppError::Unauthorized(
            "API key belum dikonfigurasi di server (set TELECORE_API_KEY)".to_string(),
        )
        .into_response();
    }
    AppError::Unauthorized("X-API-Key tidak valid atau tidak ada".to_string()).into_response()
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// Rate limit sederhana in-memory: N request per menit per IP
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct RateLimiter {
    max_per_minute: usize,
    inner: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
}

impl RateLimiter {
    pub fn new(max_per_minute: usize) -> Self {
        Self {
            max_per_minute,
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// `true` bila request diizinkan (dan dicatat), `false` bila sudah melebihi batas.
    pub fn check(&self, ip: &str) -> bool {
        let mut map = self.inner.lock().unwrap();
        let now = Instant::now();
        let entry = map.entry(ip.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        if entry.len() >= self.max_per_minute {
            return false;
        }
        entry.push(now);
        true
    }
}

pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .or_else(|| {
            req.headers()
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.split(',').next().unwrap_or("unknown").trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());
    if !state.rate_limiter.check(&ip) {
        return AppError::RateLimited.into_response();
    }
    next.run(req).await
}

// ---------------------------------------------------------------------------
// Redaksi log: jangan pernah menulis token/secret ke log.
// ---------------------------------------------------------------------------

/// Masking nilai sensitif dalam string log. Implementasi terpusat di
/// `security::redact` agar pola redaksi yang sama dipakai error Telegram,
/// notifikasi, activity log, dan serialisasi API.
pub fn sanitize_log(input: &str) -> String {
    crate::security::redact::redact_secrets(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_blocks_after_limit() {
        let limiter = RateLimiter::new(3);
        assert!(limiter.check("1.2.3.4"));
        assert!(limiter.check("1.2.3.4"));
        assert!(limiter.check("1.2.3.4"));
        assert!(!limiter.check("1.2.3.4"));
        // IP lain tidak terpengaruh.
        assert!(limiter.check("5.6.7.8"));
    }

    #[test]
    fn sanitize_masks_secrets() {
        let fake = format!("token={}:{}rahasia{}", "123456789", "A".repeat(20), "B".repeat(10));
        let s = sanitize_log(&fake);
        assert!(!s.contains("123456"));
        assert!(!s.contains("rahasia"));
        assert!(s.contains("****"));
        let s2 = sanitize_log(&format!("error dari bot {}:{}", "987654321", "C".repeat(35)));
        assert!(!s2.contains("987654321"));
    }
}
