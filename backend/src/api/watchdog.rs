//! Watchdog API (v1.2): status cek bot + pengaturan interval/auto-restart.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{AppError, AppResult, Envelope};
use crate::models::Bot;
use crate::services::watchdog::{
    set_setting, watchdog_auto_restart, watchdog_interval_minutes,
    WATCHDOG_DEFAULT_INTERVAL_MINUTES,
};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct WatchdogBotStatus {
    pub bot_id: String,
    pub username: Option<String>,
    pub status: String,
    pub last_check_at: Option<String>,
    pub consecutive_failures: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WatchdogStatusResponse {
    pub last_check_at: Option<String>,
    pub interval_minutes: i64,
    pub auto_restart: bool,
    pub bots: Vec<WatchdogBotStatus>,
}

pub async fn get_status(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<WatchdogStatusResponse>>> {
    let interval = watchdog_interval_minutes(&state).await;
    let auto_restart = watchdog_auto_restart(&state).await;
    let bots = sqlx::query_as::<_, Bot>(
        "SELECT * FROM bots WHERE is_active = 1 ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await?;
    let last_check_at = bots
        .iter()
        .filter_map(|b| b.last_check_at.clone())
        .max();
    let mut out = Vec::new();
    for bot in bots {
        // Error terakhir dari activity log watchdog bot ini.
        let last_error: Option<(String,)> = sqlx::query_as(
            "SELECT details FROM activity_logs \
             WHERE bot_id = ? AND action = 'watchdog_failure' \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&bot.id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
        let last_error = last_error.and_then(|(details,)| {
            serde_json::from_str::<serde_json::Value>(&details)
                .ok()?
                .get("error")
                .and_then(|e| e.as_str())
                .map(|s| s.to_string())
        });
        out.push(WatchdogBotStatus {
            bot_id: bot.id.clone(),
            username: bot.username.clone(),
            status: bot.status.clone(),
            last_check_at: bot.last_check_at.clone(),
            consecutive_failures: bot.consecutive_failures,
            last_error,
        });
    }
    Ok(Envelope::ok(WatchdogStatusResponse {
        last_check_at,
        interval_minutes: interval,
        auto_restart,
        bots: out,
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateWatchdogSettingsRequest {
    pub interval_minutes: Option<i64>,
    pub auto_restart: Option<bool>,
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(req): Json<UpdateWatchdogSettingsRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    if let Some(v) = req.interval_minutes {
        if !(1..=60).contains(&v) {
            return Err(AppError::Validation(
                "interval_minutes harus 1..60".to_string(),
            ));
        }
        set_setting(&state, "watchdog_interval_minutes", &v.to_string()).await;
    }
    if let Some(v) = req.auto_restart {
        set_setting(&state, "watchdog_auto_restart", if v { "1" } else { "0" }).await;
    }
    Ok(Envelope::ok(json!({
        "interval_minutes": watchdog_interval_minutes(&state).await,
        "auto_restart": watchdog_auto_restart(&state).await,
    })))
}

#[allow(dead_code)]
fn _default_interval() -> i64 {
    WATCHDOG_DEFAULT_INTERVAL_MINUTES
}
