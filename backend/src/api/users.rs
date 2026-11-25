//! Pengguna bot: list/search, detail + aktivitas, notes/labels,
//! block/unblock, export CSV.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::error::{AppError, AppResult, Envelope};
use crate::models::{
    now_rfc3339, ActivityLog, BotUser, Message, Paginated, Pagination,
};
use crate::services::log_activity;
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct ListUsersQuery {
    pub bot_id: Option<String>,
    pub q: Option<String>,
    pub is_blocked: Option<bool>,
    #[serde(flatten)]
    pub pagination: Pagination,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub notes: Option<String>,
    pub labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ExportUsersQuery {
    pub bot_id: Option<String>,
    pub format: Option<String>,
}

async fn fetch_user(state: &AppState, id: &str) -> AppResult<BotUser> {
    sqlx::query_as::<_, BotUser>("SELECT * FROM bot_users WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("user {id} tidak ditemukan")))
}

pub async fn list_users(
    State(state): State<AppState>,
    Query(q): Query<ListUsersQuery>,
) -> AppResult<Json<Envelope<Paginated<BotUser>>>> {
    let (page, per_page) = q.pagination.normalized();
    let offset = q.pagination.offset();

    let mut qb = sqlx::QueryBuilder::new("SELECT * FROM bot_users WHERE 1=1");
    if let Some(b) = &q.bot_id {
        qb.push(" AND bot_id = ").push_bind(b);
    }
    if let Some(blocked) = q.is_blocked {
        qb.push(" AND is_blocked = ").push_bind(if blocked { 1 } else { 0 });
    }
    if let Some(search) = &q.q {
        let pattern = format!("%{}%", search.trim());
        qb.push(" AND (username LIKE ")
            .push_bind(pattern.clone())
            .push(" OR first_name LIKE ")
            .push_bind(pattern.clone())
            .push(" OR last_name LIKE ")
            .push_bind(pattern)
            .push(")");
    }
    qb.push(" ORDER BY last_seen_at DESC LIMIT ")
        .push_bind(per_page)
        .push(" OFFSET ")
        .push_bind(offset);
    let items = qb.build_query_as::<BotUser>().fetch_all(&state.pool).await?;

    let mut count_qb = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM bot_users WHERE 1=1");
    if let Some(b) = &q.bot_id {
        count_qb.push(" AND bot_id = ").push_bind(b);
    }
    if let Some(blocked) = q.is_blocked {
        count_qb
            .push(" AND is_blocked = ")
            .push_bind(if blocked { 1 } else { 0 });
    }
    if let Some(search) = &q.q {
        let pattern = format!("%{}%", search.trim());
        count_qb
            .push(" AND (username LIKE ")
            .push_bind(pattern.clone())
            .push(" OR first_name LIKE ")
            .push_bind(pattern.clone())
            .push(" OR last_name LIKE ")
            .push_bind(pattern)
            .push(")");
    }
    let total = count_qb
        .build_query_scalar::<i64>()
        .fetch_one(&state.pool)
        .await?;

    Ok(Envelope::ok(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}

pub async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let user = fetch_user(&state, &id).await?;
    let recent_messages = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE bot_id = ? AND from_user_id = ? \
         ORDER BY created_at DESC LIMIT 20",
    )
    .bind(&user.bot_id)
    .bind(user.telegram_user_id)
    .fetch_all(&state.pool)
    .await?;
    let activity = sqlx::query_as::<_, ActivityLog>(
        "SELECT * FROM activity_logs WHERE bot_id = ? AND entity_id = ? \
         ORDER BY created_at DESC LIMIT 20",
    )
    .bind(&user.bot_id)
    .bind(user.telegram_user_id.to_string())
    .fetch_all(&state.pool)
    .await?;
    Ok(Envelope::ok(json!({
        "user": user,
        "recent_messages": recent_messages,
        "activity": activity,
    })))
}

pub async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateUserRequest>,
) -> AppResult<Json<Envelope<BotUser>>> {
    let mut user = fetch_user(&state, &id).await?;
    if let Some(notes) = req.notes {
        user.notes = if notes.trim().is_empty() {
            None
        } else {
            Some(notes)
        };
    }
    if let Some(labels) = req.labels {
        user.labels = serde_json::to_string(&labels).unwrap();
    }
    user.updated_at = now_rfc3339();
    sqlx::query("UPDATE bot_users SET notes = ?, labels = ?, updated_at = ? WHERE id = ?")
        .bind(&user.notes)
        .bind(&user.labels)
        .bind(&user.updated_at)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(user))
}

async fn set_blocked(state: &AppState, id: &str, blocked: bool) -> AppResult<BotUser> {
    let user = fetch_user(state, id).await?;
    sqlx::query("UPDATE bot_users SET is_blocked = ?, updated_at = ? WHERE id = ?")
        .bind(if blocked { 1 } else { 0 })
        .bind(now_rfc3339())
        .bind(id)
        .execute(&state.pool)
        .await?;
    // Catatan: block di sini adalah flag internal TeleCore (bot mengabaikan
    // pesan dari user yang diblokir di level aplikasi). Telegram Bot API
    // tidak menyediakan "block user" — user hanya bisa memblokir bot.
    log_activity(
        state,
        Some(&user.bot_id),
        if blocked { "user_blocked" } else { "user_unblocked" },
        Some("bot_user"),
        Some(&user.telegram_user_id.to_string()),
        json!({"username": user.username}),
        "info",
    )
    .await;
    fetch_user(state, id).await
}

pub async fn block_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<BotUser>>> {
    Ok(Envelope::ok(set_blocked(&state, &id, true).await?))
}

pub async fn unblock_user(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<BotUser>>> {
    Ok(Envelope::ok(set_blocked(&state, &id, false).await?))
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// Export pengguna sebagai CSV: GET /api/users/export?format=csv&bot_id=...
pub async fn export_users(
    State(state): State<AppState>,
    Query(q): Query<ExportUsersQuery>,
) -> AppResult<Response> {
    let format = q.format.as_deref().unwrap_or("csv");
    if format != "csv" {
        return Err(AppError::Validation(
            "format export yang didukung saat ini: csv".to_string(),
        ));
    }
    let users = match &q.bot_id {
        Some(bot_id) => {
            sqlx::query_as::<_, BotUser>(
                "SELECT * FROM bot_users WHERE bot_id = ? ORDER BY created_at",
            )
            .bind(bot_id)
            .fetch_all(&state.pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, BotUser>("SELECT * FROM bot_users ORDER BY created_at")
                .fetch_all(&state.pool)
                .await?
        }
    };
    let mut csv = String::from(
        "id,bot_id,telegram_user_id,username,first_name,last_name,language_code,is_blocked,message_count,last_seen_at,created_at\n",
    );
    for u in users {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{}\n",
            csv_escape(&u.id),
            csv_escape(&u.bot_id),
            u.telegram_user_id,
            csv_escape(u.username.as_deref().unwrap_or("")),
            csv_escape(u.first_name.as_deref().unwrap_or("")),
            csv_escape(u.last_name.as_deref().unwrap_or("")),
            csv_escape(u.language_code.as_deref().unwrap_or("")),
            u.is_blocked,
            u.message_count,
            csv_escape(u.last_seen_at.as_deref().unwrap_or("")),
            csv_escape(&u.created_at),
        ));
    }
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"telecore-users.csv\"",
        )
        .body(axum::body::Body::from(csv))
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(response.into_response())
}
