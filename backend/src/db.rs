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
    Ok(pool)
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
