//! Backup & restore database SQLite.
//!
//! - Create: snapshot konsisten via `VACUUM INTO`, lalu dienkripsi
//!   AES-256-GCM dengan kunci = SHA-256(passphrase), disimpan sebagai
//!   `backups/telecore-backup-<timestamp>.enc`.
//! - Restore: dekripsi -> cek header SQLite -> `PRAGMA integrity_check` pada
//!   salinan sementara -> bila valid, ditulis ke `<db>.restore-pending` dan
//!   diterapkan saat server restart berikutnya (file DB yang sedang dipakai
//!   pool tidak boleh ditimpa langsung; lihat `db::apply_pending_restore`).

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::{Path as FsPath, PathBuf};
use std::str::FromStr;

use crate::error::{AppError, AppResult, Envelope};
use crate::models::{new_id, now_rfc3339, Backup};
use crate::security::crypto::{decrypt_with_passphrase, encrypt_with_passphrase};
use crate::state::AppState;

const BACKUP_DIR: &str = "backups";
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

#[derive(Debug, Deserialize)]
pub struct CreateBackupRequest {
    pub description: Option<String>,
    pub passphrase: String,
}

#[derive(Debug, Deserialize)]
pub struct RestoreBackupRequest {
    pub passphrase: String,
}

fn backup_file_path(filename: &str) -> AppResult<PathBuf> {
    // Cegah path traversal: hanya pakai komponen nama file.
    let base = FsPath::new(filename)
        .file_name()
        .ok_or_else(|| AppError::Validation("nama file backup tidak valid".to_string()))?;
    Ok(FsPath::new(BACKUP_DIR).join(base))
}

async fn fetch_backup(state: &AppState, id: &str) -> AppResult<Backup> {
    sqlx::query_as::<_, Backup>("SELECT * FROM backups WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("backup {id} tidak ditemukan")))
}

pub async fn create_backup(
    State(state): State<AppState>,
    Json(req): Json<CreateBackupRequest>,
) -> AppResult<Json<Envelope<Backup>>> {
    if req.passphrase.len() < 8 {
        return Err(AppError::Validation(
            "passphrase minimal 8 karakter".to_string(),
        ));
    }
    std::fs::create_dir_all(BACKUP_DIR)
        .map_err(|e| AppError::Internal(format!("gagal membuat direktori backups: {e}")))?;

    // Snapshot konsisten via VACUUM INTO ke file temp (path digenerate sendiri,
    // bukan dari input user — aman dari injeksi SQL).
    let tmp_path = std::env::temp_dir().join(format!("telecore-vacuum-{}.db", new_id()));
    let tmp_str = tmp_path.to_string_lossy().replace('\'', "''");
    let vacuum_sql = format!("VACUUM INTO '{tmp_str}'");
    sqlx::query(&vacuum_sql)
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("VACUUM INTO gagal: {e}")))?;

    let db_bytes = std::fs::read(&tmp_path)
        .map_err(|e| AppError::Internal(format!("gagal membaca snapshot: {e}")))?;
    let _ = std::fs::remove_file(&tmp_path);

    let encrypted = encrypt_with_passphrase(&req.passphrase, &db_bytes)
        .map_err(|e| AppError::Internal(format!("enkripsi backup gagal: {e}")))?;
    let filename = format!(
        "telecore-backup-{}.enc",
        Utc::now().format("%Y%m%d-%H%M%S")
    );
    let path = FsPath::new(BACKUP_DIR).join(&filename);
    std::fs::write(&path, &encrypted)
        .map_err(|e| AppError::Internal(format!("gagal menulis file backup: {e}")))?;
    let size = std::fs::metadata(&path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    let now = now_rfc3339();
    let id = new_id();
    sqlx::query(
        "INSERT INTO backups (id, filename, size_bytes, description, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&filename)
    .bind(size)
    .bind(req.description.as_deref())
    .bind(&now)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(fetch_backup(&state, &id).await?))
}

pub async fn list_backups(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<Vec<Backup>>>> {
    let backups = sqlx::query_as::<_, Backup>("SELECT * FROM backups ORDER BY created_at DESC")
        .fetch_all(&state.pool)
        .await?;
    Ok(Envelope::ok(backups))
}

pub async fn restore_backup(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RestoreBackupRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let backup = fetch_backup(&state, &id).await?;
    let path = backup_file_path(&backup.filename)?;
    let encrypted = std::fs::read_to_string(&path).map_err(|_| {
        AppError::NotFound(format!("file backup {} tidak ditemukan di disk", backup.filename))
    })?;

    // 1. Dekripsi (gagal bila passphrase salah / file rusak).
    let db_bytes = decrypt_with_passphrase(&req.passphrase, encrypted.trim())
        .map_err(|e| AppError::Validation(format!("restore gagal: {e}")))?;

    // 2. Cek header SQLite.
    if db_bytes.len() < 16 || &db_bytes[..16] != SQLITE_HEADER {
        return Err(AppError::Validation(
            "restore gagal: data hasil dekripsi bukan file SQLite yang valid".to_string(),
        ));
    }

    // 3. integrity_check pada salinan sementara.
    let tmp_path = std::env::temp_dir().join(format!("telecore-restore-{}.db", new_id()));
    std::fs::write(&tmp_path, &db_bytes)
        .map_err(|e| AppError::Internal(format!("gagal menulis file restore sementara: {e}")))?;
    let check_result = async {
        let options = SqliteConnectOptions::from_str(&format!(
            "sqlite:{}",
            tmp_path.to_string_lossy()
        ))?
        .create_if_missing(false);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let result = sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await?;
        pool.close().await;
        Ok::<String, anyhow::Error>(result)
    }
    .await;
    let integrity = match check_result {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(AppError::Validation(format!(
                "restore gagal: integrity_check tidak bisa dijalankan: {e}"
            )));
        }
    };
    if integrity.trim() != "ok" {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(AppError::Validation(format!(
            "restore gagal: database backup rusak (integrity_check: {integrity})"
        )));
    }

    // 4. Stage untuk diterapkan saat restart (DB aktif tidak boleh ditimpa).
    let db_path = state.config.db_path();
    if db_path == ":memory:" {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(AppError::Validation(
            "restore tidak didukung untuk database in-memory".to_string(),
        ));
    }
    let pending = format!("{db_path}.restore-pending");
    std::fs::rename(&tmp_path, &pending)
        .map_err(|e| AppError::Internal(format!("gagal men-stage restore: {e}")))?;

    Ok(Envelope::ok(json!({
        "restored": true,
        "requires_restart": true,
        "backup_id": backup.id,
        "message": "Backup tervalidasi (dekripsi + header SQLite + integrity_check OK) dan dijadwalkan untuk diterapkan. Restart server agar database dipulihkan.",
    })))
}

pub async fn delete_backup(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let backup = fetch_backup(&state, &id).await?;
    if let Ok(path) = backup_file_path(&backup.filename) {
        let _ = std::fs::remove_file(path);
    }
    sqlx::query("DELETE FROM backups WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({"deleted": true, "id": id})))
}
