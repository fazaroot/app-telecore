//! Live logs (v1.1): GET /api/logs mengembalikan 200 baris log terakhir
//! dari ring buffer yang diisi layer tracing kustom (lihat `logging.rs`).
//! Event log juga dipublikasikan ke WebSocket hub sebagai event tipe `"log"`
//! (lihat `logging.rs::LogLayer`).

use axum::{extract::{Query, State}, Json};
use serde::Deserialize;
use serde_json::json;

use crate::error::{AppResult, Envelope};
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct LogsQuery {
    /// Jumlah baris terakhir (default 200, maks 200 — kapasitas ring buffer).
    pub limit: Option<usize>,
}

pub async fn list_logs(
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let limit = q.limit.unwrap_or(200).clamp(1, 200);
    let lines = state.recent_logs(limit);
    let count = lines.len();
    Ok(Envelope::ok(json!({
        "lines": lines,
        "count": count,
        "capacity": 200,
        "note": "Baris log terbaru ada di akhir daftar. Event log juga disiarkan lewat WebSocket /ws sebagai event tipe \"log\".",
    })))
}
