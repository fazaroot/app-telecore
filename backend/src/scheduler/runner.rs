//! Runner scheduler.
//!
//! - Loop tiap 10 detik: cari task due (`next_run_at <= now`, aktif, tidak pause).
//! - Eksekusi: kirim pesan via Telegram client.
//! - Hitung `next_run_at` berikutnya sesuai recurrence + timezone.
//! - Retry gagal maks 3x dengan backoff eksponensial (60s, 120s, 240s);
//!   setelah 3x gagal task dinonaktifkan + notifikasi dibuat.
//! - History dicatat di `activity_logs` + field task diupdate.

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use chrono_tz::Tz;
use std::str::FromStr;

use crate::api::notifications::create_notification;
use crate::bot::get_bot_token;
use crate::bot::telegram::SendMessageParams;
use crate::models::ScheduledTask;
use crate::services::log_activity;
use crate::services::outgoing_webhooks::dispatch_outgoing_event;
use crate::state::AppState;

pub const MAX_FAILURES: i64 = 3;

pub async fn scheduler_loop(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
    loop {
        interval.tick().await;
        if let Err(e) = run_due_tasks(&state).await {
            tracing::error!("Scheduler error: {e}");
        }
    }
}

pub async fn run_due_tasks(state: &AppState) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    let tasks = sqlx::query_as::<_, ScheduledTask>(
        "SELECT * FROM scheduled_tasks \
         WHERE is_active = 1 AND is_paused = 0 \
         AND next_run_at IS NOT NULL AND next_run_at <= ?",
    )
    .bind(&now)
    .fetch_all(&state.pool)
    .await?;
    for task in tasks {
        execute_task(state, &task).await;
    }
    Ok(())
}

async fn execute_task(state: &AppState, task: &ScheduledTask) {
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    let result = async {
        let token = get_bot_token(state, &task.bot_id).await?;
        state
            .telegram
            .send_message(
                &token,
                &SendMessageParams {
                    chat_id: task.chat_id,
                    text: task.message_content.clone(),
                    ..Default::default()
                },
            )
            .await?;
        Ok::<(), crate::error::AppError>(())
    }
    .await;

    match result {
        Ok(()) => {
            let next = compute_next_run(
                &task.task_type,
                task.interval_seconds,
                task.cron_expression.as_deref(),
                &task.timezone,
                now,
            );
            let (next_run_at, is_active) = match next {
                Some(n) => (Some(n.to_rfc3339()), 1),
                None => (None, 0), // one-time selesai => nonaktifkan
            };
            let _ = sqlx::query(
                "UPDATE scheduled_tasks SET last_run_at = ?, next_run_at = ?, \
                 run_count = run_count + 1, failure_count = 0, is_active = ?, updated_at = ? \
                 WHERE id = ?",
            )
            .bind(&now_str)
            .bind(next_run_at)
            .bind(is_active)
            .bind(&now_str)
            .bind(&task.id)
            .execute(&state.pool)
            .await;
            log_activity(
                state,
                Some(&task.bot_id),
                "scheduled_task_executed",
                Some("scheduled_task"),
                Some(&task.id),
                serde_json::json!({"name": task.name}),
                "info",
            )
            .await;
            state.hub.publish(
                "activity",
                serde_json::json!({"action": "scheduled_task_executed", "task_id": task.id}),
            );
            // --- Outgoing webhook: scheduled_task.completed (non-blocking) ---
            dispatch_outgoing_event(
                state,
                "scheduled_task.completed",
                Some(&task.bot_id),
                serde_json::json!({
                    "task_id": task.id,
                    "task_name": task.name,
                    "chat_id": task.chat_id,
                }),
            )
            .await;
        }
        Err(e) => {
            let failures = task.failure_count + 1;
            tracing::warn!(
                "Scheduled task {} gagal (percobaan gagal ke-{failures}): {e}",
                task.id
            );
            if failures >= MAX_FAILURES {
                let _ = sqlx::query(
                    "UPDATE scheduled_tasks SET failure_count = ?, is_active = 0, updated_at = ? WHERE id = ?",
                )
                .bind(failures)
                .bind(&now_str)
                .bind(&task.id)
                .execute(&state.pool)
                .await;
                create_notification(
                    state,
                    "scheduler_failed",
                    "Scheduled task dinonaktifkan",
                    &format!(
                        "Task '{}' gagal 3x berturut-turut dan dinonaktifkan.",
                        task.name
                    ),
                    serde_json::json!({"task_id": task.id}),
                )
                .await;
            } else {
                // Backoff: 60s * 2^(failures-1)
                let backoff_secs = 60 * 2_i64.pow((failures - 1) as u32);
                let next = (now + Duration::seconds(backoff_secs)).to_rfc3339();
                let _ = sqlx::query(
                    "UPDATE scheduled_tasks SET failure_count = ?, next_run_at = ?, updated_at = ? WHERE id = ?",
                )
                .bind(failures)
                .bind(next)
                .bind(&now_str)
                .bind(&task.id)
                .execute(&state.pool)
                .await;
            }
            log_activity(
                state,
                Some(&task.bot_id),
                "scheduled_task_failed",
                Some("scheduled_task"),
                Some(&task.id),
                serde_json::json!({"name": task.name, "failures": failures, "error": e.to_string()}),
                "error",
            )
            .await;
        }
    }
}

/// Hitung jadwal berikutnya dari `from` (pure, mudah diuji).
/// - one-time => None (tidak berulang)
/// - daily => +1 hari (jam yang sama di timezone task)
/// - weekly => +7 hari
/// - interval => +interval_seconds
/// - cron => format sederhana "MENIT JAM * * *" (harian pada jam:menit di timezone task)
pub fn compute_next_run(
    task_type: &str,
    interval_seconds: Option<i64>,
    cron_expression: Option<&str>,
    timezone: &str,
    from: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let tz: Tz = Tz::from_str(timezone).unwrap_or(chrono_tz::UTC);
    match task_type {
        "one-time" => None,
        "daily" => {
            // Bila ada cron "M H * * *", jalankan pada jam itu setiap hari;
            // bila tidak ada, hari berikutnya pada waktu yang sama.
            if let Some(expr) = cron_expression {
                if let Some(next) = next_cron_occurrence(expr, &tz, from) {
                    return Some(next);
                }
            }
            let local = from.with_timezone(&tz) + Duration::days(1);
            Some(local.with_timezone(&Utc))
        }
        "weekly" => {
            // Bila ada cron "M H * * DOW", jalankan pada hari+jam itu tiap minggu;
            // bila tidak ada, minggu berikutnya pada waktu yang sama.
            if let Some(expr) = cron_expression {
                if let Some(next) = next_cron_occurrence(expr, &tz, from) {
                    return Some(next);
                }
            }
            let local = from.with_timezone(&tz) + Duration::weeks(1);
            Some(local.with_timezone(&Utc))
        }
        "interval" => {
            let secs = interval_seconds.unwrap_or(3600).max(60);
            Some(from + Duration::seconds(secs))
        }
        "cron" => {
            let expr = cron_expression?;
            next_cron_occurrence(expr, &tz, from)
        }
        _ => None,
    }
}

/// Kemunculan berikutnya dari cron "MENIT JAM * * DOW" (DOW `*` atau 0–6,
/// 0 = Minggu; 7 juga diterima sebagai Minggu) di timezone task.
/// Mengembalikan None bila format tidak didukung.
fn next_cron_occurrence(expr: &str, tz: &Tz, from: DateTime<Utc>) -> Option<DateTime<Utc>> {
    use chrono::{Datelike, TimeZone};
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 || parts[2] != "*" || parts[3] != "*" {
        return None;
    }
    let minute: u32 = parts[0].parse().ok()?;
    let hour: u32 = parts[1].parse().ok()?;
    if minute > 59 || hour > 23 {
        return None;
    }
    let dow: Option<u32> = if parts[4] == "*" {
        None
    } else {
        let d: u32 = parts[4].parse().ok()?;
        if d > 7 {
            return None;
        }
        Some(if d == 7 { 0 } else { d })
    };
    let local_now = from.with_timezone(tz);
    let at_time = |date: chrono::NaiveDate| {
        tz.with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0)
            .single()
    };
    match dow {
        None => {
            let mut candidate = at_time(local_now.date_naive())?;
            if candidate <= local_now {
                candidate += Duration::days(1);
            }
            Some(candidate.with_timezone(&Utc))
        }
        Some(target) => {
            let current = local_now.weekday().num_days_from_sunday();
            let mut days_ahead = (target + 7 - current) % 7;
            let mut candidate = at_time(local_now.date_naive() + Duration::days(days_ahead as i64))?;
            if candidate <= local_now {
                days_ahead += 7;
                candidate = at_time(local_now.date_naive() + Duration::days(days_ahead as i64))?;
            }
            Some(candidate.with_timezone(&Utc))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn next_run_one_time_is_none() {
        assert!(compute_next_run("one-time", None, None, "UTC", t(2026, 11, 25, 10, 0)).is_none());
    }

    #[test]
    fn next_run_daily_adds_one_day() {
        let next = compute_next_run("daily", None, None, "UTC", t(2026, 11, 25, 10, 0)).unwrap();
        assert_eq!(next, t(2026, 11, 26, 10, 0));
    }

    #[test]
    fn next_run_weekly_adds_seven_days() {
        let next = compute_next_run("weekly", None, None, "UTC", t(2026, 11, 25, 10, 0)).unwrap();
        assert_eq!(next, t(2026, 12, 2, 10, 0));
    }

    #[test]
    fn next_run_interval_adds_seconds() {
        let next =
            compute_next_run("interval", Some(3600), None, "UTC", t(2026, 11, 25, 10, 0)).unwrap();
        assert_eq!(next, t(2026, 11, 25, 11, 0));
        // Interval di bawah 60 detik dibulatkan ke 60.
        let next = compute_next_run("interval", Some(5), None, "UTC", t(2026, 11, 25, 10, 0)).unwrap();
        assert_eq!(next, t(2026, 11, 25, 10, 1));
    }

    #[test]
    fn next_run_cron_same_day_and_next_day() {
        // Cron "30 9 * * *": dari 08:00 => hari ini 09:30 UTC.
        let next = compute_next_run("cron", None, Some("30 9 * * *"), "UTC", t(2026, 11, 25, 8, 0))
            .unwrap();
        assert_eq!(next, t(2026, 11, 25, 9, 30));
        // Dari 10:00 => besok 09:30.
        let next = compute_next_run("cron", None, Some("30 9 * * *"), "UTC", t(2026, 11, 25, 10, 0))
            .unwrap();
        assert_eq!(next, t(2026, 11, 26, 9, 30));
        // Format tidak didukung => None.
        assert!(compute_next_run("cron", None, Some("*/5 * * * *"), "UTC", t(2026, 11, 25, 8, 0))
            .is_none());
    }

    #[test]
    fn next_run_daily_with_cron_uses_cron_time() {
        // Daily + cron "0 9 * * *": dari 08:00 => hari ini 09:00 (bukan +24 jam).
        let next = compute_next_run("daily", None, Some("0 9 * * *"), "UTC", t(2026, 11, 25, 8, 0))
            .unwrap();
        assert_eq!(next, t(2026, 11, 25, 9, 0));
        // Dari 10:00 (jam sudah lewat) => besok 09:00.
        let next = compute_next_run("daily", None, Some("0 9 * * *"), "UTC", t(2026, 11, 25, 10, 0))
            .unwrap();
        assert_eq!(next, t(2026, 11, 26, 9, 0));
        // Tanpa cron => tetap +1 hari pada waktu yang sama.
        let next = compute_next_run("daily", None, None, "UTC", t(2026, 11, 25, 10, 0)).unwrap();
        assert_eq!(next, t(2026, 11, 26, 10, 0));
    }

    #[test]
    fn next_run_weekly_with_cron_uses_weekday() {
        // 2026-11-25 adalah Rabu (DOW 3). Weekly Senin (DOW 1) jam 09:00
        // dari Rabu 08:00 => Senin berikutnya 2026-11-30 09:00.
        let next = compute_next_run(
            "weekly",
            None,
            Some("0 9 * * 1"),
            "UTC",
            t(2026, 11, 25, 8, 0),
        )
        .unwrap();
        assert_eq!(next, t(2026, 11, 30, 9, 0));
        // Weekly Rabu (DOW 3) jam 09:00 dari Rabu 08:00 => hari ini 09:00.
        let next = compute_next_run(
            "weekly",
            None,
            Some("0 9 * * 3"),
            "UTC",
            t(2026, 11, 25, 8, 0),
        )
        .unwrap();
        assert_eq!(next, t(2026, 11, 25, 9, 0));
        // Dari Rabu 10:00 (jam sudah lewat) => Rabu minggu depan.
        let next = compute_next_run(
            "weekly",
            None,
            Some("0 9 * * 3"),
            "UTC",
            t(2026, 11, 25, 10, 0),
        )
        .unwrap();
        assert_eq!(next, t(2026, 12, 2, 9, 0));
    }

    #[test]
    fn next_run_cron_with_weekday() {
        // Cron dengan DOW: "0 9 * * 1" dari Rabu => Senin berikutnya.
        let next = compute_next_run("cron", None, Some("0 9 * * 1"), "UTC", t(2026, 11, 25, 8, 0))
            .unwrap();
        assert_eq!(next, t(2026, 11, 30, 9, 0));
    }

    #[test]
    fn next_run_daily_respects_timezone() {
        // 2026-11-25 10:00 UTC = 17:00 WIB (Asia/Jakarta). Daily berikutnya
        // harus 17:00 WIB besok = 10:00 UTC besok.
        let next = compute_next_run("daily", None, None, "Asia/Jakarta", t(2026, 11, 25, 10, 0))
            .unwrap();
        assert_eq!(next, t(2026, 11, 26, 10, 0));
    }
}
