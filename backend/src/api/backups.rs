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
use chrono::{Timelike, Utc};
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
    let backup = create_backup_internal(&state, &req.passphrase, req.description.as_deref(), false).await?;
    Ok(Envelope::ok(backup))
}

/// Inti pembuatan backup (dipakai endpoint manual DAN auto-backup v1.2).
/// Snapshot konsisten via `VACUUM INTO`, enkripsi AES-256-GCM dengan
/// kunci = SHA-256(passphrase), simpan file + baris DB.
pub async fn create_backup_internal(
    state: &AppState,
    passphrase: &str,
    description: Option<&str>,
    is_automatic: bool,
) -> AppResult<Backup> {
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

    let encrypted = encrypt_with_passphrase(passphrase, &db_bytes)
        .map_err(|e| AppError::Internal(format!("enkripsi backup gagal: {e}")))?;
    let filename = format!(
        "telecore-backup-{}{}.enc",
        if is_automatic { "auto-" } else { "" },
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
        "INSERT INTO backups (id, filename, size_bytes, description, is_automatic, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&filename)
    .bind(size)
    .bind(description)
    .bind(if is_automatic { 1 } else { 0 })
    .bind(&now)
    .execute(&state.pool)
    .await?;
    fetch_backup(state, &id).await
}

// ---------------------------------------------------------------------------
// Auto-backup terjadwal (v1.2)
// ---------------------------------------------------------------------------

/// Apakah auto-backup jatuh tempo (fungsi murni, diuji unit).
/// - daily: jam UTC sekarang == hour DAN belum ada backup hari ini.
/// - weekly: jam UTC sekarang == hour DAN backup terakhir >= 7 hari lalu
///   (atau belum pernah).
pub fn auto_backup_due(
    enabled: bool,
    frequency: &str,
    hour: u32,
    last: Option<chrono::DateTime<Utc>>,
    now: chrono::DateTime<Utc>,
) -> bool {
    if !enabled {
        return false;
    }
    if now.hour() != hour {
        return false;
    }
    let Some(last) = last else { return true };
    match frequency {
        "weekly" => now.signed_duration_since(last).num_days() >= 7,
        _ => last.date_naive() != now.date_naive(), // daily (default)
    }
}

/// ID yang harus dihapus dari daftar terurut terbaru-dulu (fungsi murni).
/// Pertahankan `retention` terbaru, hapus sisanya.
pub fn retention_delete_ids<'a>(ids_newest_first: &'a [String], retention: usize) -> Vec<&'a String> {
    if ids_newest_first.len() <= retention {
        return Vec::new();
    }
    ids_newest_first[retention..].iter().collect()
}

async fn get_auto_setting(state: &AppState, key: &str, default: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| default.to_string())
}

async fn set_auto_setting(state: &AppState, key: &str, value: &str) {
    let _ = sqlx::query(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?, ?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now_rfc3339())
    .execute(&state.pool)
    .await;
}

#[derive(Debug, Deserialize)]
pub struct AutoBackupSettingsRequest {
    pub enabled: Option<bool>,
    pub frequency: Option<String>, // daily | weekly
    pub hour: Option<i64>,        // 0..23 (UTC)
    pub retention: Option<i64>,
}

#[derive(Debug, serde::Serialize)]
pub struct AutoBackupSettingsResponse {
    pub enabled: bool,
    pub frequency: String,
    pub hour: i64,
    pub retention: i64,
    pub last_auto_backup_at: Option<String>,
}

pub async fn get_auto_settings(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<AutoBackupSettingsResponse>>> {
    Ok(Envelope::ok(AutoBackupSettingsResponse {
        enabled: get_auto_setting(&state, "auto_backup_enabled", "0").await == "1",
        frequency: get_auto_setting(&state, "auto_backup_frequency", "daily").await,
        hour: get_auto_setting(&state, "auto_backup_hour", "3")
            .await
            .parse()
            .unwrap_or(3),
        retention: get_auto_setting(&state, "auto_backup_retention", "7")
            .await
            .parse()
            .unwrap_or(7),
        last_auto_backup_at: sqlx::query_scalar::<_, String>(
            "SELECT value FROM app_settings WHERE key = 'last_auto_backup_at'",
        )
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten(),
    }))
}

pub async fn update_auto_settings(
    State(state): State<AppState>,
    Json(req): Json<AutoBackupSettingsRequest>,
) -> AppResult<Json<Envelope<AutoBackupSettingsResponse>>> {
    if let Some(f) = &req.frequency {
        if f != "daily" && f != "weekly" {
            return Err(AppError::Validation(
                "frequency harus 'daily' atau 'weekly'".to_string(),
            ));
        }
        set_auto_setting(&state, "auto_backup_frequency", f).await;
    }
    if let Some(h) = req.hour {
        if !(0..=23).contains(&h) {
            return Err(AppError::Validation("hour harus 0..23".to_string()));
        }
        set_auto_setting(&state, "auto_backup_hour", &h.to_string()).await;
    }
    if let Some(r) = req.retention {
        if r < 1 {
            return Err(AppError::Validation("retention minimal 1".to_string()));
        }
        set_auto_setting(&state, "auto_backup_retention", &r.to_string()).await;
    }
    if let Some(e) = req.enabled {
        set_auto_setting(&state, "auto_backup_enabled", if e { "1" } else { "0" }).await;
    }
    get_auto_settings(State(state)).await
}

/// Dipanggil scheduler tiap tick: bila jatuh tempo, jalankan backup
/// otomatis + prune retensi. Guard in-flight (static AtomicBool) mencegah
/// dobel saat backup berjalan lebih lama dari interval tick 10 detik.
pub async fn maybe_run_auto_backup(state: &AppState) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static RUNNING: AtomicBool = AtomicBool::new(false);
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let result = maybe_run_auto_backup_inner(state).await;
    RUNNING.store(false, Ordering::SeqCst);
    if let Err(e) = result {
        tracing::warn!("Auto-backup gagal: {e}");
    }
}

async fn maybe_run_auto_backup_inner(state: &AppState) -> anyhow::Result<()> {
    let enabled = get_auto_setting(state, "auto_backup_enabled", "0").await == "1";
    if !enabled {
        return Ok(());
    }
    let frequency = get_auto_setting(state, "auto_backup_frequency", "daily").await;
    let hour: u32 = get_auto_setting(state, "auto_backup_hour", "3")
        .await
        .parse()
        .unwrap_or(3);
    let retention: usize = get_auto_setting(state, "auto_backup_retention", "7")
        .await
        .parse()
        .unwrap_or(7);
    let last: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT value FROM app_settings WHERE key = 'last_auto_backup_at'",
    )
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let last_dt = last
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let now = Utc::now();
    if !auto_backup_due(enabled, &frequency, hour, last_dt, now) {
        return Ok(());
    }
    // Passphrase: pakai yang tersimpan (terenkripsi master key); bila belum
    // ada, generate sekali dan simpan terenkripsi.
    let passphrase = match get_auto_setting(state, "auto_backup_passphrase_enc", "").await {
        enc if !enc.is_empty() => state.crypto.decrypt(&enc)?,
        _ => {
            use rand::RngCore;
            let mut bytes = [0u8; 16];
            rand::rngs::OsRng.fill_bytes(&mut bytes);
            let plain = hex::encode(bytes);
            let enc = state.crypto.encrypt(&plain)?;
            set_auto_setting(state, "auto_backup_passphrase_enc", &enc).await;
            plain
        }
    };
    // Tandai waktu SEBELUM menjalankan backup agar tick berikutnya dalam
    // jam yang sama tidak memicu dobel (backup jalan >10 detik).
    set_auto_setting(state, "last_auto_backup_at", &now.to_rfc3339()).await;
    let backup = create_backup_internal(state, &passphrase, Some("Auto-backup terjadwal"), true)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    tracing::info!("Auto-backup selesai: {}", backup.filename);
    crate::services::log_activity(
        state,
        None,
        "auto_backup_completed",
        Some("backup"),
        Some(&backup.id),
        serde_json::json!({"filename": backup.filename, "frequency": frequency}),
        "info",
    )
    .await;
    // Prune retensi: hanya generasi otomatis, terbaru dulu.
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, filename FROM backups WHERE is_automatic = 1 ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    let ids: Vec<String> = rows.iter().map(|(id, _)| id.clone()).collect();
    let to_delete = retention_delete_ids(&ids, retention);
    for id in to_delete {
        if let Some((_, filename)) = rows.iter().find(|(i, _)| i == id) {
            let _ = std::fs::remove_file(FsPath::new(BACKUP_DIR).join(filename));
        }
        let _ = sqlx::query("DELETE FROM backups WHERE id = ?")
            .bind(id)
            .execute(&state.pool)
            .await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(y: i32, mo: u32, d: u32, h: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, 0, 0).unwrap()
    }

    #[test]
    fn auto_backup_due_daily() {
        // Jam cocok, belum pernah backup => due.
        assert!(auto_backup_due(true, "daily", 3, None, t(2026, 11, 25, 3)));
        // Jam tidak cocok => tidak due.
        assert!(!auto_backup_due(true, "daily", 3, None, t(2026, 11, 25, 4)));
        // Sudah backup hari ini pada jam yang sama => tidak due.
        assert!(!auto_backup_due(
            true,
            "daily",
            3,
            Some(t(2026, 11, 25, 3)),
            t(2026, 11, 25, 3)
        ));
        // Backup kemarin => due.
        assert!(auto_backup_due(
            true,
            "daily",
            3,
            Some(t(2026, 11, 24, 3)),
            t(2026, 11, 25, 3)
        ));
        // Disabled => tidak pernah due.
        assert!(!auto_backup_due(false, "daily", 3, None, t(2026, 11, 25, 3)));
    }

    #[test]
    fn auto_backup_due_weekly() {
        // Terakhir 6 hari lalu => belum due.
        assert!(!auto_backup_due(
            true,
            "weekly",
            3,
            Some(t(2026, 11, 19, 3)),
            t(2026, 11, 25, 3)
        ));
        // Terakhir 7 hari lalu => due.
        assert!(auto_backup_due(
            true,
            "weekly",
            3,
            Some(t(2026, 11, 18, 3)),
            t(2026, 11, 25, 3)
        ));
    }

    #[test]
    fn retention_keeps_n_newest_and_deletes_rest() {
        let ids: Vec<String> = (0..5).map(|i| format!("b{i}")).collect();
        // ids sudah urut terbaru dulu: b0 terbaru.
        let del = retention_delete_ids(&ids, 2);
        assert_eq!(del, vec![&"b2".to_string(), &"b3".to_string(), &"b4".to_string()]);
        assert!(retention_delete_ids(&ids, 7).is_empty());
        assert!(retention_delete_ids(&ids, 5).is_empty());
        assert_eq!(retention_delete_ids(&ids, 0).len(), 5);
    }
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
