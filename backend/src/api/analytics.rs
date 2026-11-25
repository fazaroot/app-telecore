//! Analytics: ringkasan + export CSV/JSON.
//!
//! Catatan jujur: `avg_response_time_ms` bernilai `null` di v1 karena waktu
//! respons bot (selisih pesan masuk -> balasan) belum dilacak. Field tetap
//! ada agar kontrak API stabil.

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{AppError, AppResult, Envelope};
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct SummaryQuery {
    pub bot_id: Option<String>,
    /// RFC3339, mis. 2026-11-01T00:00:00Z
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ExportQuery {
    pub bot_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub format: Option<String>, // csv | json
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct DayCount {
    day: String,
    inbound: i64,
    outbound: i64,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct StatusCount {
    status: String,
    count: i64,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
struct CommandUsage {
    name: String,
    usage_count: i64,
}

fn apply_filters<'a>(
    qb: &mut sqlx::QueryBuilder<'a, sqlx::Sqlite>,
    bot_id: &'a Option<String>,
    from: &'a Option<String>,
    to: &'a Option<String>,
) {
    if let Some(b) = bot_id {
        qb.push(" AND bot_id = ").push_bind(b);
    }
    if let Some(f) = from {
        qb.push(" AND created_at >= ").push_bind(f);
    }
    if let Some(t) = to {
        qb.push(" AND created_at <= ").push_bind(t);
    }
}

pub async fn compute_summary(
    state: &AppState,
    bot_id: &Option<String>,
    from: &Option<String>,
    to: &Option<String>,
) -> AppResult<serde_json::Value> {
    // Pesan per hari, dipisah inbound/outbound (untuk grafik traffic).
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT date(created_at) AS day, \
         COALESCE(SUM(CASE WHEN direction = 'inbound' THEN 1 ELSE 0 END), 0) AS inbound, \
         COALESCE(SUM(CASE WHEN direction = 'outbound' THEN 1 ELSE 0 END), 0) AS outbound \
         FROM messages WHERE 1=1",
    );
    apply_filters(&mut qb, bot_id, from, to);
    qb.push(" GROUP BY day ORDER BY day");
    let per_day = qb.build_query_as::<DayCount>().fetch_all(&state.pool).await?;

    // Total pesan + gagal.
    let mut qb = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM messages WHERE 1=1");
    apply_filters(&mut qb, bot_id, from, to);
    let total_messages = qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?;

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT COUNT(*) FROM messages WHERE direction = 'inbound' AND 1=1",
    );
    apply_filters(&mut qb, bot_id, from, to);
    let inbound = qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?;

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT COUNT(*) FROM messages WHERE direction = 'outbound' AND 1=1",
    );
    apply_filters(&mut qb, bot_id, from, to);
    let outbound = qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?;

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT COUNT(*) FROM messages WHERE status = 'failed' AND 1=1",
    );
    apply_filters(&mut qb, bot_id, from, to);
    let failed = qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?;

    // User aktif (distinct pengirim pesan masuk) & user baru.
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT COUNT(DISTINCT from_user_id) FROM messages \
         WHERE direction = 'inbound' AND from_user_id IS NOT NULL AND 1=1",
    );
    apply_filters(&mut qb, bot_id, from, to);
    let active_users = qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?;

    let mut qb = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM bot_users WHERE 1=1");
    apply_filters(&mut qb, bot_id, from, to);
    let new_users = qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?;

    // Total pengguna terdaftar (tanpa filter tanggal).
    let mut qb = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM bot_users WHERE 1=1");
    if let Some(b) = bot_id {
        qb.push(" AND bot_id = ").push_bind(b);
    }
    let total_users = qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?;

    // Performa automation.
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT status, COUNT(*) AS count FROM automation_executions WHERE 1=1",
    );
    apply_filters(&mut qb, bot_id, from, to);
    qb.push(" GROUP BY status");
    let automation = qb
        .build_query_as::<StatusCount>()
        .fetch_all(&state.pool)
        .await?;

    // Penggunaan command.
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT name, usage_count FROM bot_commands WHERE usage_count > 0 AND 1=1",
    );
    if let Some(b) = bot_id {
        qb.push(" AND bot_id = ").push_bind(b);
    }
    qb.push(" ORDER BY usage_count DESC LIMIT 10");
    let command_usage = qb
        .build_query_as::<CommandUsage>()
        .fetch_all(&state.pool)
        .await?;

    Ok(json!({
        "messages_per_day": per_day,
        "totals": {
            "messages": total_messages,
            "inbound": inbound,
            "outbound": outbound,
            "failed": failed,
        },
        "total_users": total_users,
        "active_users": active_users,
        "new_users": new_users,
        "automation_by_status": automation,
        "command_usage": command_usage,
        "avg_response_time_ms": null,
        "note": "avg_response_time_ms belum dilacak di v1 (null).",
    }))
}

pub async fn summary(
    State(state): State<AppState>,
    Query(q): Query<SummaryQuery>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let data = compute_summary(&state, &q.bot_id, &q.from, &q.to).await?;
    Ok(Envelope::ok(data))
}

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct PerBotRow {
    pub bot_id: String,
    pub bot_name: String,
    pub messages_in: i64,
    pub messages_out: i64,
    pub active_users: i64,
    pub new_users: i64,
    pub automations_run: i64,
    /// Pesan gagal + eksekusi automation gagal dalam rentang.
    pub failed: i64,
}

/// Hitung statistik per bot dari tabel yang sudah ada.
/// - `messages_in/out`: tabel `messages` (direction).
/// - `active_users`: distinct `from_user_id` pesan inbound.
/// - `new_users`: baris `bot_users` yang dibuat dalam rentang.
/// - `automations_run`: semua eksekusi di `automation_executions`.
/// - `failed`: pesan `status='failed'` + eksekusi automation `status='failed'`.
pub async fn compute_per_bot(
    state: &AppState,
    from: &Option<String>,
    to: &Option<String>,
) -> AppResult<Vec<PerBotRow>> {
    let bots = sqlx::query_as::<_, (String, String)>(
        "SELECT id, name FROM bots ORDER BY created_at",
    )
    .fetch_all(&state.pool)
    .await?;
    let mut out = Vec::new();
    for (bot_id, bot_name) in bots {
        let messages_in = {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT COUNT(*) FROM messages WHERE direction = 'inbound'",
            );
            qb.push(" AND bot_id = ").push_bind(&bot_id);
            if let Some(f) = from {
                qb.push(" AND created_at >= ").push_bind(f);
            }
            if let Some(t) = to {
                qb.push(" AND created_at <= ").push_bind(t);
            }
            qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?
        };
        let messages_out = {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT COUNT(*) FROM messages WHERE direction = 'outbound'",
            );
            qb.push(" AND bot_id = ").push_bind(&bot_id);
            if let Some(f) = from {
                qb.push(" AND created_at >= ").push_bind(f);
            }
            if let Some(t) = to {
                qb.push(" AND created_at <= ").push_bind(t);
            }
            qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?
        };
        let failed_messages = {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT COUNT(*) FROM messages WHERE status = 'failed'",
            );
            qb.push(" AND bot_id = ").push_bind(&bot_id);
            if let Some(f) = from {
                qb.push(" AND created_at >= ").push_bind(f);
            }
            if let Some(t) = to {
                qb.push(" AND created_at <= ").push_bind(t);
            }
            qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?
        };
        let active_users = {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT COUNT(DISTINCT from_user_id) FROM messages \
                 WHERE direction = 'inbound' AND from_user_id IS NOT NULL",
            );
            qb.push(" AND bot_id = ").push_bind(&bot_id);
            if let Some(f) = from {
                qb.push(" AND created_at >= ").push_bind(f);
            }
            if let Some(t) = to {
                qb.push(" AND created_at <= ").push_bind(t);
            }
            qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?
        };
        let new_users = {
            let mut qb =
                sqlx::QueryBuilder::new("SELECT COUNT(*) FROM bot_users WHERE 1=1");
            qb.push(" AND bot_id = ").push_bind(&bot_id);
            if let Some(f) = from {
                qb.push(" AND created_at >= ").push_bind(f);
            }
            if let Some(t) = to {
                qb.push(" AND created_at <= ").push_bind(t);
            }
            qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?
        };
        let automations_run = {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT COUNT(*) FROM automation_executions WHERE 1=1",
            );
            qb.push(" AND bot_id = ").push_bind(&bot_id);
            if let Some(f) = from {
                qb.push(" AND created_at >= ").push_bind(f);
            }
            if let Some(t) = to {
                qb.push(" AND created_at <= ").push_bind(t);
            }
            qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?
        };
        let failed_automations = {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT COUNT(*) FROM automation_executions WHERE status = 'failed'",
            );
            qb.push(" AND bot_id = ").push_bind(&bot_id);
            if let Some(f) = from {
                qb.push(" AND created_at >= ").push_bind(f);
            }
            if let Some(t) = to {
                qb.push(" AND created_at <= ").push_bind(t);
            }
            qb.build_query_scalar::<i64>().fetch_one(&state.pool).await?
        };
        out.push(PerBotRow {
            bot_id,
            bot_name,
            messages_in,
            messages_out,
            active_users,
            new_users,
            automations_run,
            failed: failed_messages + failed_automations,
        });
    }
    Ok(out)
}

pub async fn per_bot(
    State(state): State<AppState>,
    Query(q): Query<SummaryQuery>,
) -> AppResult<Json<Envelope<Vec<PerBotRow>>>> {
    let rows = compute_per_bot(&state, &q.from, &q.to).await?;
    Ok(Envelope::ok(rows))
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Export per-bot analytics sebagai CSV.
pub async fn per_bot_export(
    State(state): State<AppState>,
    Query(q): Query<SummaryQuery>,
) -> AppResult<Response> {
    let rows = compute_per_bot(&state, &q.from, &q.to).await?;
    let mut csv = String::from(
        "bot_id,bot_name,messages_in,messages_out,active_users,new_users,automations_run,failed\n",
    );
    for r in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            csv_escape(&r.bot_id),
            csv_escape(&r.bot_name),
            r.messages_in,
            r.messages_out,
            r.active_users,
            r.new_users,
            r.automations_run,
            r.failed,
        ));
    }
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"telecore-analytics-per-bot.csv\"",
        )
        .body(axum::body::Body::from(csv))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(response)
}

/// Export analytics: GET /api/analytics/export?format=csv|json
pub async fn export(
    State(state): State<AppState>,
    Query(q): Query<ExportQuery>,
) -> AppResult<Response> {
    let format = q.format.as_deref().unwrap_or("json");
    let data = compute_summary(&state, &q.bot_id, &q.from, &q.to).await?;
    match format {
        "json" => {
            let body = Envelope::ok(data);
            Ok((StatusCode::OK, body).into_response())
        }
        "csv" => {
            let mut csv = String::from("day,inbound,outbound,total\n");
            if let Some(days) = data.get("messages_per_day").and_then(|v| v.as_array()) {
                for d in days {
                    let day = d.get("day").and_then(|v| v.as_str()).unwrap_or("");
                    let inbound = d.get("inbound").and_then(|v| v.as_i64()).unwrap_or(0);
                    let outbound = d.get("outbound").and_then(|v| v.as_i64()).unwrap_or(0);
                    csv.push_str(&format!("{day},{inbound},{outbound},{}\n", inbound + outbound));
                }
            }
            let response = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
                .header(
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"telecore-analytics.csv\"",
                )
                .body(axum::body::Body::from(csv))
                .map_err(|e| AppError::Internal(e.to_string()))?;
            Ok(response)
        }
        other => Err(AppError::Validation(format!(
            "format tidak dikenal: '{other}' (gunakan csv atau json)"
        ))),
    }
}
