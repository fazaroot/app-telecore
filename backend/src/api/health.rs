//! GET /api/health — tanpa auth.

use axum::{extract::State, Json};
use serde_json::json;

use crate::error::Envelope;
use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Json<Envelope<serde_json::Value>> {
    let db_ok = sqlx::query("SELECT 1")
        .execute(&state.pool)
        .await
        .is_ok();
    Envelope::ok(json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "database": if db_ok { "ok" } else { "error" },
        "uptime_seconds": state.started_at.elapsed().as_secs(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
