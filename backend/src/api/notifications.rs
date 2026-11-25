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
///
/// Semua teks dan metadata JSON diredaksi di sini (terpusat) sebelum
/// disimpan/disiarkan, agar pemanggil mana pun tidak bisa membocorkan
/// token bot lewat pesan error.
pub async fn create_notification(
    state: &AppState,
    notification_type: &str,
    title: &str,
    message: &str,
    data: serde_json::Value,
) {
    let notification_type = crate::security::redact::redact_secrets(notification_type);
    let title = crate::security::redact::redact_secrets(title);
    let message = crate::security::redact::redact_secrets(message);
    let mut data = data;
    crate::security::redact::redact_json_value(&mut data);
    let id = new_id();
    let now = now_rfc3339();
    let _ = sqlx::query(
        "INSERT INTO notifications (id, type, title, message, data, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&notification_type)
    .bind(&title)
    .bind(&message)
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Query, State};

    fn synthetic_token() -> String {
        format!("123456789:{}", "FAKE".repeat(9))
    }

    async fn test_state() -> AppState {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(":memory:")
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let config = crate::config::Config {
            host: "127.0.0.1".to_string(),
            port: 8080,
            database_url: "sqlite::memory:".to_string(),
            api_key: "test-key".to_string(),
            rust_log: "info".to_string(),
        };
        let crypto = std::sync::Arc::new(crate::security::Crypto::from_key_bytes([9u8; 32]));
        let telegram = crate::bot::telegram::TelegramClient::new().unwrap();
        AppState::new(pool, config, crypto, telegram)
    }

    #[tokio::test]
    async fn create_notification_redacts_token_in_message_and_data() {
        let state = test_state().await;
        let token = synthetic_token();
        create_notification(
            &state,
            "watchdog_failure",
            "Bot tidak merespons getMe",
            &format!("gagal: https://api.telegram.org/bot{token}/getMe"),
            json!({"error": format!("bot {token} gagal"), "token": "pendek"}),
        )
        .await;
        let (message, data): (String, String) =
            sqlx::query_as("SELECT message, data FROM notifications LIMIT 1")
                .fetch_one(&state.pool)
                .await
                .unwrap();
        assert!(!message.contains(&token));
        assert!(!data.contains(&token));
        assert!(!data.contains("pendek"));
        // Output API juga tidak mengandung token.
        let Json(envelope) = list_notifications(
            State(state.clone()),
            Query(ListNotificationsQuery::default()),
        )
        .await
        .unwrap();
        let serialized = serde_json::to_string(&envelope).unwrap();
        assert!(!serialized.contains(&token));
    }

    #[tokio::test]
    async fn legacy_notification_row_is_redacted_in_api_output() {
        let state = test_state().await;
        let token = synthetic_token();
        // Baris lama yang sudah terlanjur menyimpan token (insert langsung).
        sqlx::query(
            "INSERT INTO notifications (id, type, title, message, data, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("legacy-1")
        .bind("watchdog_failure")
        .bind("Bot tidak merespons getMe")
        .bind(format!("error lama https://api.telegram.org/bot{token}/getMe"))
        .bind(json!({"error": format!("bot {token}")}).to_string())
        .bind(now_rfc3339())
        .execute(&state.pool)
        .await
        .unwrap();
        let Json(envelope) = list_notifications(
            State(state.clone()),
            Query(ListNotificationsQuery::default()),
        )
        .await
        .unwrap();
        let serialized = serde_json::to_string(&envelope).unwrap();
        assert!(!serialized.contains(&token));
        assert!(serialized.contains("****"));
    }
}
