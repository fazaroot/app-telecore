//! Error aplikasi + envelope respons JSON yang konsisten.
//!
//! Sukses:  `{"success": true, "data": ...}`
//! Gagal:   `{"success": false, "error": {"code": "...", "message": "..."}}`

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl<T: Serialize> Envelope<T> {
    pub fn ok(data: T) -> Json<Envelope<T>> {
        Json(Envelope {
            success: true,
            data: Some(data),
            error: None,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Telegram(String),
    #[error("rate limited")]
    RateLimited,
    #[error("{0}")]
    Internal(String),
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("{0}")]
    Anyhow(#[from] anyhow::Error),
}

impl AppError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            AppError::Validation(_) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR"),
            AppError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "FORBIDDEN"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT"),
            AppError::Telegram(_) => (StatusCode::BAD_GATEWAY, "TELEGRAM_ERROR"),
            AppError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
            AppError::Sqlx(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR"),
            AppError::Anyhow(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        // Log server-side; pesan ke klien tetap generik untuk error 500.
        // Semua teks error diredaksi sebelum di-log/dikirim agar token
        // bot (mis. dari URL reqwest) tidak pernah bocor.
        if status.is_server_error() {
            tracing::error!("{code}: {}", crate::security::redact::redact_secrets(&self.to_string()));
        }
        let message = match &self {
            AppError::Sqlx(_) | AppError::Internal(_) | AppError::Anyhow(_) => {
                "Terjadi kesalahan internal server".to_string()
            }
            _ => crate::security::redact::redact_secrets(&self.to_string()),
        };
        let body = Envelope::<serde_json::Value> {
            success: false,
            data: None,
            error: Some(ErrorBody {
                code: code.to_string(),
                message,
            }),
        };
        (status, Json(body)).into_response()
    }
}
