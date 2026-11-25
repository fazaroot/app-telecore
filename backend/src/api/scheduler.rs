//! Scheduler: CRUD task, pause/resume, history.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use serde::Deserialize;
use serde_json::json;
use std::str::FromStr;

use crate::error::{AppError, AppResult, Envelope};
use crate::models::{new_id, now_rfc3339, ActivityLog, Paginated, Pagination, ScheduledTask};
use crate::scheduler::runner::compute_next_run;
use crate::state::AppState;

pub const TASK_TYPES: &[&str] = &["one-time", "daily", "weekly", "interval", "cron"];

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub bot_id: String,
    pub name: String,
    pub description: Option<String>,
    pub task_type: String,
    #[serde(deserialize_with = "crate::models::de_chat_id")]
    pub chat_id: i64,
    pub message_content: String,
    pub media_url: Option<String>,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub timezone: Option<String>,
    /// Jadwal jalan pertama (RFC3339). Bila kosong, dihitung dari recurrence.
    pub next_run_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTaskRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub task_type: Option<String>,
    #[serde(default, deserialize_with = "crate::models::de_opt_chat_id")]
    pub chat_id: Option<i64>,
    pub message_content: Option<String>,
    pub media_url: Option<String>,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub timezone: Option<String>,
    pub next_run_at: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListTasksQuery {
    pub bot_id: Option<String>,
    #[serde(flatten)]
    pub pagination: Pagination,
}

fn validate_timezone(tz: &str) -> AppResult<()> {
    if Tz::from_str(tz).is_err() {
        return Err(AppError::Validation(format!(
            "timezone tidak dikenal: '{tz}' (contoh: UTC, Asia/Jakarta)"
        )));
    }
    Ok(())
}

fn parse_rfc3339(s: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| {
            AppError::Validation(format!("waktu tidak valid (harus RFC3339): '{s}'"))
        })
}

async fn fetch_task(state: &AppState, id: &str) -> AppResult<ScheduledTask> {
    sqlx::query_as::<_, ScheduledTask>("SELECT * FROM scheduled_tasks WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("task {id} tidak ditemukan")))
}

pub async fn list_tasks(
    State(state): State<AppState>,
    Query(q): Query<ListTasksQuery>,
) -> AppResult<Json<Envelope<Paginated<ScheduledTask>>>> {
    let (page, per_page) = q.pagination.normalized();
    let offset = q.pagination.offset();
    let (items, total) = match &q.bot_id {
        Some(bot_id) => (
            sqlx::query_as::<_, ScheduledTask>(
                "SELECT * FROM scheduled_tasks WHERE bot_id = ? ORDER BY next_run_at LIMIT ? OFFSET ?",
            )
            .bind(bot_id)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM scheduled_tasks WHERE bot_id = ?")
                .bind(bot_id)
                .fetch_one(&state.pool)
                .await?,
        ),
        None => (
            sqlx::query_as::<_, ScheduledTask>(
                "SELECT * FROM scheduled_tasks ORDER BY next_run_at LIMIT ? OFFSET ?",
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM scheduled_tasks")
                .fetch_one(&state.pool)
                .await?,
        ),
    };
    Ok(Envelope::ok(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}

pub async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> AppResult<Json<Envelope<ScheduledTask>>> {
    if req.bot_id.trim().is_empty() {
        return Err(AppError::Validation("bot_id tidak boleh kosong".to_string()));
    }
    if req.name.trim().is_empty() {
        return Err(AppError::Validation("nama task tidak boleh kosong".to_string()));
    }
    if req.chat_id == 0 {
        return Err(AppError::Validation("chat_id tidak boleh kosong".to_string()));
    }
    if req.message_content.trim().is_empty() {
        return Err(AppError::Validation(
            "message_content tidak boleh kosong".to_string(),
        ));
    }
    if !TASK_TYPES.contains(&req.task_type.as_str()) {
        return Err(AppError::Validation(format!(
            "task_type tidak dikenal: '{}' (yang didukung: {})",
            req.task_type,
            TASK_TYPES.join(", ")
        )));
    }
    let timezone = req.timezone.clone().unwrap_or_else(|| "UTC".to_string());
    validate_timezone(&timezone)?;
    if req.task_type == "cron" {
        let expr = req.cron_expression.as_deref().unwrap_or("");
        if compute_next_run("cron", None, Some(expr), &timezone, Utc::now()).is_none() {
            return Err(AppError::Validation(
                "cron_expression tidak valid — format yang didukung: \"MENIT JAM * * *\" (mis. \"30 9 * * *\")".to_string(),
            ));
        }
    }
    if req.task_type == "interval" {
        let secs = req.interval_seconds.unwrap_or(0);
        if secs < 60 {
            return Err(AppError::Validation(
                "interval_seconds minimal 60 detik".to_string(),
            ));
        }
    }
    let bot_exists: Option<(String,)> = sqlx::query_as("SELECT id FROM bots WHERE id = ?")
        .bind(&req.bot_id)
        .fetch_optional(&state.pool)
        .await?;
    if bot_exists.is_none() {
        return Err(AppError::NotFound(format!(
            "bot {} tidak ditemukan",
            req.bot_id
        )));
    }

    // Jadwal jalan pertama.
    let next_run_at: String = match &req.next_run_at {
        Some(s) => parse_rfc3339(s)?.to_rfc3339(),
        None => {
            if req.task_type == "one-time" {
                return Err(AppError::Validation(
                    "one-time task wajib menyertakan next_run_at".to_string(),
                ));
            }
            compute_next_run(
                &req.task_type,
                req.interval_seconds,
                req.cron_expression.as_deref(),
                &timezone,
                Utc::now(),
            )
            .map(|dt| dt.to_rfc3339())
            .ok_or_else(|| {
                AppError::Validation("tidak bisa menghitung jadwal berikutnya".to_string())
            })?
        }
    };

    let now = now_rfc3339();
    let id = new_id();
    sqlx::query(
        "INSERT INTO scheduled_tasks (id, bot_id, name, description, task_type, chat_id, \
         message_content, media_url, cron_expression, interval_seconds, timezone, next_run_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.bot_id)
    .bind(req.name.trim())
    .bind(req.description.as_deref())
    .bind(&req.task_type)
    .bind(req.chat_id)
    .bind(req.message_content.trim())
    .bind(req.media_url.as_deref())
    .bind(req.cron_expression.as_deref())
    .bind(req.interval_seconds)
    .bind(&timezone)
    .bind(&next_run_at)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(fetch_task(&state, &id).await?))
}

pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<ScheduledTask>>> {
    Ok(Envelope::ok(fetch_task(&state, &id).await?))
}

pub async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> AppResult<Json<Envelope<ScheduledTask>>> {
    let mut task = fetch_task(&state, &id).await?;
    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err(AppError::Validation("nama task tidak boleh kosong".to_string()));
        }
        task.name = name.trim().to_string();
    }
    if let Some(d) = req.description {
        task.description = Some(d);
    }
    if let Some(t) = req.task_type {
        if !TASK_TYPES.contains(&t.as_str()) {
            return Err(AppError::Validation(format!("task_type tidak dikenal: '{t}'")));
        }
        task.task_type = t;
    }
    if let Some(c) = req.chat_id {
        if c == 0 {
            return Err(AppError::Validation("chat_id tidak boleh kosong".to_string()));
        }
        task.chat_id = c;
    }
    if let Some(m) = req.message_content {
        if m.trim().is_empty() {
            return Err(AppError::Validation(
                "message_content tidak boleh kosong".to_string(),
            ));
        }
        task.message_content = m;
    }
    if let Some(m) = req.media_url {
        task.media_url = Some(m);
    }
    if let Some(c) = req.cron_expression {
        task.cron_expression = Some(c);
    }
    if let Some(i) = req.interval_seconds {
        if i < 60 {
            return Err(AppError::Validation(
                "interval_seconds minimal 60 detik".to_string(),
            ));
        }
        task.interval_seconds = Some(i);
    }
    if let Some(tz) = req.timezone {
        validate_timezone(&tz)?;
        task.timezone = tz;
    }
    if let Some(n) = req.next_run_at {
        task.next_run_at = Some(parse_rfc3339(&n)?.to_rfc3339());
    }
    if let Some(a) = req.is_active {
        task.is_active = if a { 1 } else { 0 };
    }
    task.updated_at = now_rfc3339();
    sqlx::query(
        "UPDATE scheduled_tasks SET name = ?, description = ?, task_type = ?, chat_id = ?, \
         message_content = ?, media_url = ?, cron_expression = ?, interval_seconds = ?, \
         timezone = ?, next_run_at = ?, is_active = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&task.name)
    .bind(&task.description)
    .bind(&task.task_type)
    .bind(task.chat_id)
    .bind(&task.message_content)
    .bind(&task.media_url)
    .bind(&task.cron_expression)
    .bind(task.interval_seconds)
    .bind(&task.timezone)
    .bind(&task.next_run_at)
    .bind(task.is_active)
    .bind(&task.updated_at)
    .bind(&task.id)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(task))
}

pub async fn delete_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    fetch_task(&state, &id).await?;
    sqlx::query("DELETE FROM scheduled_tasks WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({"deleted": true, "id": id})))
}

async fn set_paused(state: &AppState, id: &str, paused: bool) -> AppResult<ScheduledTask> {
    fetch_task(state, id).await?;
    sqlx::query("UPDATE scheduled_tasks SET is_paused = ?, updated_at = ? WHERE id = ?")
        .bind(if paused { 1 } else { 0 })
        .bind(now_rfc3339())
        .bind(id)
        .execute(&state.pool)
        .await?;
    fetch_task(state, id).await
}

pub async fn pause_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<ScheduledTask>>> {
    Ok(Envelope::ok(set_paused(&state, &id, true).await?))
}

pub async fn resume_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<ScheduledTask>>> {
    Ok(Envelope::ok(set_paused(&state, &id, false).await?))
}

/// History eksekusi scheduler dari activity_logs.
pub async fn task_history(
    State(state): State<AppState>,
    Query(q): Query<ListTasksQuery>,
) -> AppResult<Json<Envelope<Paginated<ActivityLog>>>> {
    let (page, per_page) = q.pagination.normalized();
    let offset = q.pagination.offset();
    let (items, total) = match &q.bot_id {
        Some(bot_id) => (
            sqlx::query_as::<_, ActivityLog>(
                "SELECT * FROM activity_logs WHERE entity_type = 'scheduled_task' AND bot_id = ? \
                 ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(bot_id)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM activity_logs WHERE entity_type = 'scheduled_task' AND bot_id = ?",
            )
            .bind(bot_id)
            .fetch_one(&state.pool)
            .await?,
        ),
        None => (
            sqlx::query_as::<_, ActivityLog>(
                "SELECT * FROM activity_logs WHERE entity_type = 'scheduled_task' \
                 ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM activity_logs WHERE entity_type = 'scheduled_task'",
            )
            .fetch_one(&state.pool)
            .await?,
        ),
    };
    Ok(Envelope::ok(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}

/// Riwayat eksekusi untuk SATU task: GET /scheduler/:id/history.
/// Difilter dari activity_logs (entity_type='scheduled_task', entity_id=task id).
pub async fn task_history_for_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(p): Query<Pagination>,
) -> AppResult<Json<Envelope<Paginated<ActivityLog>>>> {
    // Pastikan task ada (404 bila tidak).
    sqlx::query_as::<_, ScheduledTask>("SELECT * FROM scheduled_tasks WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("task {id} tidak ditemukan")))?;
    let (page, per_page) = p.normalized();
    let offset = p.offset();
    let items = sqlx::query_as::<_, ActivityLog>(
        "SELECT * FROM activity_logs WHERE entity_type = 'scheduled_task' AND entity_id = ? \
         ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(&id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM activity_logs WHERE entity_type = 'scheduled_task' AND entity_id = ?",
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
