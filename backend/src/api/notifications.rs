//! Notifikasi in-app: list, mark read, mark all read, delete.
//! Juga menyediakan `create_notification` untuk modul lain (scheduler, dll).

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::error::{AppError, AppResult, Envelope};
use crate::models::{new_id, now_rfc3339, Notification, Paginated, Pagination};
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct ListNotificationsQuery {
    pub is_read: Option<bool>,
    #[serde(flatten)]
    pub pagination: Pagination,
}

/// Buat notifikasi + broadcast ke WebSocket.
pub async fn create_notification(
    state: &AppState,
    notification_type: &str,
    title: &str,
    message: &str,
    data: serde_json::Value,
) {
    let id = new_id();
    let now = now_rfc3339();
    let _ = sqlx::query(
        "INSERT INTO notifications (id, type, title, message, data, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(notification_type)
    .bind(title)
    .bind(message)
    .bind(data.to_string())
    .bind(&now)
    .execute(&state.pool)
    .await;
    state.hub.publish(
        "notification",
        json!({"id": id, "type": notification_type, "title": title, "message": message}),
    );
}

pub async fn list_notifications(
    State(state): State<AppState>,
    Query(q): Query<ListNotificationsQuery>,
) -> AppResult<Json<Envelope<Paginated<Notification>>>> {
    let (page, per_page) = q.pagination.normalized();
    let offset = q.pagination.offset();
    let (items, total) = match q.is_read {
        Some(read) => (
            sqlx::query_as::<_, Notification>(
                "SELECT * FROM notifications WHERE is_read = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(if read { 1 } else { 0 })
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM notifications WHERE is_read = ?")
                .bind(if read { 1 } else { 0 })
                .fetch_one(&state.pool)
                .await?,
        ),
        None => (
            sqlx::query_as::<_, Notification>(
                "SELECT * FROM notifications ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM notifications")
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

pub async fn mark_read(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<Notification>>> {
    let result = sqlx::query("UPDATE notifications SET is_read = 1 WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "notifikasi {id} tidak ditemukan"
        )));
    }
    let n = sqlx::query_as::<_, Notification>("SELECT * FROM notifications WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Envelope::ok(n))
}

pub async fn mark_all_read(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let result = sqlx::query("UPDATE notifications SET is_read = 1 WHERE is_read = 0")
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({
        "marked_read": result.rows_affected(),
    })))
}

pub async fn delete_notification(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let result = sqlx::query("DELETE FROM notifications WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "notifikasi {id} tidak ditemukan"
        )));
    }
    Ok(Envelope::ok(json!({"deleted": true, "id": id})))
}
