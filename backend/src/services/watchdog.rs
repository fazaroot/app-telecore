//! Bot Watchdog (v1.2).
//!
//! Background task: tiap interval (default 5 menit, dari app_settings
//! `watchdog_interval_minutes` dan `watchdog_auto_restart`) memanggil
//! `getMe` untuk setiap bot aktif. Bila gagal: notifikasi + activity log,
//! `consecutive_failures` +1, `last_check_at` diupdate; bila auto-restart
//! aktif, task polling bot itu dihentikan lalu dijalankan ulang (maks 3
//! percobaan, jeda 10 detik). Bila tetap gagal setelah percobaan:
//! `bots.connection_status = 'error'`. Bila sukses: failures direset 0,
//! `last_check_at` diupdate, status kembali 'online' bila sebelumnya 'error'.
//!
//! Freshness polling: bila bot memakai polling, waktu update terakhir
//! (last_update_offset) tidak cukup untuk menilai freshness tanpa timestamp
//! update — jadi bila tidak ada data freshness, bot dianggap ok selama
//! getMe sukses (sesuai spek v1.2). Restart memakai PollingManager yang
//! sudah ada: `stop` (abort task lama) lalu `start` (task baru).

use std::time::Duration;

use crate::api::notifications::create_notification;
use crate::bot::get_bot_token;
use crate::models::{now_rfc3339, Bot};
use crate::services::log_activity;
use crate::state::AppState;

pub const WATCHDOG_DEFAULT_INTERVAL_MINUTES: i64 = 5;
pub const WATCHDOG_MAX_RESTART_ATTEMPTS: u32 = 3;
pub const WATCHDOG_RESTART_DELAY_SECS: u64 = 10;

/// Hasil penerapan satu hasil cek ke state failure bot (fungsi murni).
/// Mengembalikan (failures_baru, status_baru_atau_None).
/// - Sukses: failures 0; status 'online' bila sebelumnya 'error', else None.
/// - Gagal: failures+1; status None (tetap), kecuali bila `exhausted`
///   (restart sudah dicoba 3x dan tetap gagal) => Some("error").
pub fn apply_check_result(
    success: bool,
    previous_failures: i64,
    previous_status: &str,
    exhausted: bool,
) -> (i64, Option<&'static str>) {
    if success {
        let status = if previous_status == "error" {
            Some("online")
        } else {
            None
        };
        (0, status)
    } else if exhausted {
        (previous_failures + 1, Some("error"))
    } else {
        (previous_failures + 1, None)
    }
}

pub async fn get_setting(state: &AppState, key: &str, default: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT value FROM app_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| default.to_string())
}

pub async fn set_setting(state: &AppState, key: &str, value: &str) {
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

pub async fn watchdog_interval_minutes(state: &AppState) -> i64 {
    get_setting(
        state,
        "watchdog_interval_minutes",
        &WATCHDOG_DEFAULT_INTERVAL_MINUTES.to_string(),
    )
    .await
    .parse::<i64>()
    .unwrap_or(WATCHDOG_DEFAULT_INTERVAL_MINUTES)
    .clamp(1, 60)
}

pub async fn watchdog_auto_restart(state: &AppState) -> bool {
    get_setting(state, "watchdog_auto_restart", "0").await == "1"
}

/// Loop background watchdog: cek, lalu tidur sepanjang interval
/// (interval dibaca ulang setiap putaran agar perubahan setting berlaku).
pub async fn watchdog_loop(state: AppState) {
    // Beri waktu server start + polling jalan sebelum cek pertama.
    tokio::time::sleep(Duration::from_secs(60)).await;
    loop {
        if let Err(e) = check_all_bots(&state).await {
            tracing::error!("Watchdog error: {e}");
        }
        let interval = watchdog_interval_minutes(&state).await;
        tokio::time::sleep(Duration::from_secs((interval.max(1) * 60) as u64)).await;
    }
}

pub async fn check_all_bots(state: &AppState) -> anyhow::Result<()> {
    let bots = sqlx::query_as::<_, Bot>(
        "SELECT * FROM bots WHERE is_active = 1",
    )
    .fetch_all(&state.pool)
    .await?;
    let auto_restart = watchdog_auto_restart(state).await;
    for bot in bots {
        check_one_bot(state, &bot, auto_restart).await;
    }
    Ok(())
}

async fn check_one_bot(state: &AppState, bot: &Bot, auto_restart: bool) {
    let now = now_rfc3339();
    let token = match get_bot_token(state, &bot.id).await {
        Ok(t) => t,
        Err(e) => {
            record_failure(state, bot, &format!("gagal mendekripsi token: {e}"), auto_restart)
                .await;
            return;
        }
    };
    match state.telegram.get_me(&token).await {
        Ok(_) => {
            let (failures, new_status) =
                apply_check_result(true, bot.consecutive_failures, &bot.status, false);
            let status = new_status.unwrap_or(&bot.status);
            let _ = sqlx::query(
                "UPDATE bots SET last_check_at = ?, consecutive_failures = ?, status = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&now)
            .bind(failures)
            .bind(status)
            .bind(&now)
            .bind(&bot.id)
            .execute(&state.pool)
            .await;
        }
        Err(e) => {
            record_failure(state, bot, &e.to_string(), auto_restart).await;
        }
    }
}

async fn record_failure(state: &AppState, bot: &Bot, error: &str, auto_restart: bool) {
    let now = now_rfc3339();
    // Restart polling (bila aktif): maks 3 percobaan, jeda 10 detik.
    // `PollingManager::start` sudah menghentikan task lama dulu.
    let mut restarted_ok = false;
    if auto_restart {
        let polling_enabled: Option<(i64,)> = sqlx::query_as(
            "SELECT polling_enabled FROM bot_configurations WHERE bot_id = ?",
        )
        .bind(&bot.id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
        if polling_enabled.map(|(v,)| v).unwrap_or(0) == 1 {
            for attempt in 1..=WATCHDOG_MAX_RESTART_ATTEMPTS {
                state.polling.start(state.clone(), &bot.id).await;
                // Verifikasi: getMe lagi setelah restart.
                if let Ok(token) = get_bot_token(state, &bot.id).await {
                    if state.telegram.get_me(&token).await.is_ok() {
                        restarted_ok = true;
                        log_activity(
                            state,
                            Some(&bot.id),
                            "watchdog_restarted",
                            Some("bot"),
                            Some(&bot.id),
                            serde_json::json!({"attempt": attempt}),
                            "info",
                        )
                        .await;
                        break;
                    }
                }
                if attempt < WATCHDOG_MAX_RESTART_ATTEMPTS {
                    tokio::time::sleep(Duration::from_secs(WATCHDOG_RESTART_DELAY_SECS)).await;
                }
            }
        }
    }
    let (failures, new_status) = apply_check_result(
        false,
        bot.consecutive_failures,
        &bot.status,
        auto_restart && !restarted_ok,
    );
    let status = new_status.unwrap_or(&bot.status);
    let _ = sqlx::query(
        "UPDATE bots SET last_check_at = ?, consecutive_failures = ?, status = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&now)
    .bind(failures)
    .bind(status)
    .bind(&now)
    .bind(&bot.id)
    .execute(&state.pool)
    .await;
    create_notification(
        state,
        "watchdog_failure",
        "Bot tidak merespons getMe",
        &format!(
            "Bot '{}' gagal dicek watchdog (kegagalan berurutan: {failures}): {error}",
            bot.name
        ),
        serde_json::json!({"bot_id": bot.id, "failures": failures, "error": error}),
    )
    .await;
    log_activity(
        state,
        Some(&bot.id),
        "watchdog_failure",
        Some("bot"),
        Some(&bot.id),
        serde_json::json!({"failures": failures, "error": error, "restarted": restarted_ok}),
        "error",
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_resets_failures_and_restores_online_from_error() {
        assert_eq!(apply_check_result(true, 3, "error", false), (0, Some("online")));
        assert_eq!(apply_check_result(true, 0, "online", false), (0, None));
        assert_eq!(apply_check_result(true, 2, "disconnected", false), (0, None));
    }

    #[test]
    fn failure_increments_without_changing_status() {
        assert_eq!(apply_check_result(false, 0, "online", false), (1, None));
        assert_eq!(apply_check_result(false, 2, "online", false), (3, None));
    }

    #[test]
    fn exhausted_failure_marks_error() {
        assert_eq!(
            apply_check_result(false, 2, "online", true),
            (3, Some("error"))
        );
    }
}
