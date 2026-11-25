//! Koneksi SQLite (SQLx) + migrasi otomatis saat start.

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;

use crate::config::db_path_from_url;

/// Buat pool SQLite dan jalankan migrasi (`sqlx::migrate!`, embedded).
pub async fn init_pool(database_url: &str) -> anyhow::Result<SqlitePool> {
    let path = db_path_from_url(database_url);
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    // Pembersihan baris lama: redaksi pola token bot yang terlanjur
    // tersimpan di notifikasi/activity sebelum sanitasi terpusat ada.
    // Idempoten (aman di setiap startup, termasuk setelah restore backup)
    // dan tidak menghapus baris pengguna.
    let redacted = redact_stored_secrets(&pool).await?;
    if redacted > 0 {
        tracing::info!("Redaksi token lama: {redacted} baris notifikasi/activity dibersihkan.");
    }
    Ok(pool)
}

/// Redaksi pola token bot/secret pada teks tersimpan di `notifications`
/// dan `activity_logs`. Mengembalikan jumlah baris yang diubah.
pub async fn redact_stored_secrets(pool: &SqlitePool) -> anyhow::Result<usize> {
    use crate::security::redact::{redact_json_text, redact_secrets};

    let mut changed = 0usize;

    let notifications = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT id, title, message, data FROM notifications",
    )
    .fetch_all(pool)
    .await?;
    for (id, title, message, data) in notifications {
        let new_title = redact_secrets(&title);
        let new_message = redact_secrets(&message);
        let new_data = redact_json_text(&data);
        if new_title != title || new_message != message || new_data != data {
            sqlx::query("UPDATE notifications SET title = ?, message = ?, data = ? WHERE id = ?")
                .bind(&new_title)
                .bind(&new_message)
                .bind(&new_data)
                .bind(&id)
                .execute(pool)
                .await?;
            changed += 1;
        }
    }

    let activities = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String)>(
        "SELECT id, action, entity_type, entity_id, details FROM activity_logs",
    )
    .fetch_all(pool)
    .await?;
    for (id, action, entity_type, entity_id, details) in activities {
        let new_action = redact_secrets(&action);
        let new_entity_type = entity_type.as_deref().map(redact_secrets);
        let new_entity_id = entity_id.as_deref().map(redact_secrets);
        let new_details = redact_json_text(&details);
        if new_action != action
            || new_entity_type != entity_type
            || new_entity_id != entity_id
            || new_details != details
        {
            sqlx::query(
                "UPDATE activity_logs SET action = ?, entity_type = ?, entity_id = ?, details = ? WHERE id = ?",
            )
            .bind(&new_action)
            .bind(new_entity_type.as_deref())
            .bind(new_entity_id.as_deref())
            .bind(&new_details)
            .bind(&id)
            .execute(pool)
            .await?;
            changed += 1;
        }
    }

    Ok(changed)
}

/// Terapkan restore backup yang tertunda (jika ada).
///
/// Restore ditulis ke `<db>.restore-pending` oleh endpoint restore karena file
/// DB yang sedang dipakai pool tidak boleh ditimpa. Saat server start berikutnya,
/// file pending ini menggantikan DB lama (file -wal/-shm lama dihapus).
/// Dipanggil SEBELUM `init_pool`.
pub fn apply_pending_restore(db_path: &str) -> anyhow::Result<()> {
    if db_path == ":memory:" {
        return Ok(());
    }
    let pending = format!("{db_path}.restore-pending");
    if !Path::new(&pending).exists() {
        return Ok(());
    }
    tracing::warn!("Restore backup tertunda ditemukan — mengganti database dengan file hasil restore.");
    let _ = std::fs::remove_file(format!("{db_path}-wal"));
    let _ = std::fs::remove_file(format!("{db_path}-shm"));
    std::fs::rename(&pending, db_path)?;
    tracing::info!("Database berhasil dipulihkan dari backup.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn redact_stored_secrets_cleans_legacy_rows_without_deleting() {
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
        let token = format!("123456789:{}", "FAKE".repeat(9));
        let now = crate::models::now_rfc3339();
        sqlx::query(
            "INSERT INTO notifications (id, type, title, message, data, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("n1")
        .bind("watchdog_failure")
        .bind("Bot tidak merespons getMe")
        .bind(format!("error https://api.telegram.org/bot{token}/getMe"))
        .bind(serde_json::json!({"error": format!("bot {token}")}).to_string())
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO activity_logs (id, bot_id, action, entity_type, entity_id, details, level, created_at) \
             VALUES (?, NULL, ?, ?, ?, ?, ?, ?)",
        )
        .bind("a1")
        .bind("watchdog_failure")
        .bind("bot")
        .bind("bot-1")
        .bind(serde_json::json!({"error": format!("bot {token}")}).to_string())
        .bind("error")
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        let changed = redact_stored_secrets(&pool).await.unwrap();
        assert_eq!(changed, 2);

        // Baris tetap ada, token hilang, JSON tetap valid.
        let (message, data): (String, String) =
            sqlx::query_as("SELECT message, data FROM notifications WHERE id = 'n1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!message.contains(&token));
        assert!(!data.contains(&token));
        assert!(serde_json::from_str::<serde_json::Value>(&data).is_ok());
        let details: String =
            sqlx::query_scalar("SELECT details FROM activity_logs WHERE id = 'a1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!details.contains(&token));
        assert!(serde_json::from_str::<serde_json::Value>(&details).is_ok());
        // Idempoten: jalan kedua tidak mengubah apa pun.
        assert_eq!(redact_stored_secrets(&pool).await.unwrap(), 0);
    }
}
