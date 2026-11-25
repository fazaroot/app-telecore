//! Automation engine.
//!
//! Trigger: new_user, keyword, command, message_received, scheduled,
//! user_joined_group.
//! Condition (JSON): `{ "logic": "AND"|"OR", "rules": [{ "field", "operator",
//! "value" }] }` atau array rules langsung (dianggap AND).
//! Operator: eq/equals, ne/not_equals, contains, not_contains, starts_with,
//! ends_with, gt/greater_than, lt/less_than.
//! Action: send_message, send_template, add_label, save_note, delay,
//! save_activity, notify_admin.
//!
//! Loop guard: maks 10 eksekusi per workflow per menit, maks 20 langkah per
//! eksekusi, pesan dari bot sendiri diabaikan (dicek di `process_update`).

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::api::notifications::create_notification;
use crate::bot::get_bot_token;
use crate::bot::telegram::SendMessageParams;
use crate::models::{new_id, now_rfc3339, AutomationWorkflow, MessageTemplate};
use crate::services::log_activity;
use crate::services::outgoing_webhooks::dispatch_outgoing_event;
use crate::state::AppState;

pub const MAX_EXECUTIONS_PER_MINUTE: usize = 10;
pub const MAX_STEPS_PER_EXECUTION: usize = 20;

/// Loop guard: hitung eksekusi per workflow dalam jendela 60 detik.
pub struct AutomationGuard {
    counts: Mutex<HashMap<String, Vec<Instant>>>,
}

impl AutomationGuard {
    pub fn new() -> Self {
        Self {
            counts: Mutex::new(HashMap::new()),
        }
    }

    /// `true` bila eksekusi diizinkan (dan dicatat), `false` bila sudah
    /// mencapai batas 10/menit.
    pub fn allow(&self, workflow_id: &str) -> bool {
        let mut map = self.counts.lock().unwrap();
        let now = Instant::now();
        let entry = map.entry(workflow_id.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        if entry.len() >= MAX_EXECUTIONS_PER_MINUTE {
            return false;
        }
        entry.push(now);
        true
    }
}

impl Default for AutomationGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Evaluasi kondisi (pure, mudah diuji)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ConditionRule {
    pub field: String,
    pub operator: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ConditionsObject {
    #[serde(default = "default_logic")]
    logic: String,
    #[serde(default)]
    rules: Vec<ConditionRule>,
}

fn default_logic() -> String {
    "AND".to_string()
}

/// Evaluasi kondisi terhadap konteks event. Kondisi kosong => `true`.
pub fn evaluate_conditions(conditions_json: &str, context: &serde_json::Value) -> bool {
    let trimmed = conditions_json.trim();
    if trimmed.is_empty() || trimmed == "[]" || trimmed == "{}" || trimmed == "null" {
        return true;
    }
    // Coba parse sebagai objek {logic, rules} dulu, lalu sebagai array rules.
    if let Ok(obj) = serde_json::from_str::<ConditionsObject>(trimmed) {
        return evaluate_rules(&obj.rules, &obj.logic, context);
    }
    if let Ok(rules) = serde_json::from_str::<Vec<ConditionRule>>(trimmed) {
        return evaluate_rules(&rules, "AND", context);
    }
    // JSON tidak valid => kondisi dianggap tidak cocok (fail-closed).
    false
}

fn evaluate_rules(rules: &[ConditionRule], logic: &str, context: &serde_json::Value) -> bool {
    if rules.is_empty() {
        return true;
    }
    let results: Vec<bool> = rules.iter().map(|r| evaluate_rule(r, context)).collect();
    if logic.eq_ignore_ascii_case("OR") {
        results.iter().any(|b| *b)
    } else {
        results.iter().all(|b| *b)
    }
}

pub fn evaluate_rule(rule: &ConditionRule, context: &serde_json::Value) -> bool {
    let actual = context.get(&rule.field);
    match rule.operator.to_lowercase().as_str() {
        "eq" | "equals" => actual == Some(&rule.value),
        "ne" | "not_equals" => actual != Some(&rule.value),
        "contains" => match (actual.and_then(|v| v.as_str()), rule.value.as_str()) {
            (Some(a), Some(b)) => a.to_lowercase().contains(&b.to_lowercase()),
            _ => false,
        },
        "not_contains" => match (actual.and_then(|v| v.as_str()), rule.value.as_str()) {
            (Some(a), Some(b)) => !a.to_lowercase().contains(&b.to_lowercase()),
            _ => false,
        },
        "starts_with" => match (actual.and_then(|v| v.as_str()), rule.value.as_str()) {
            (Some(a), Some(b)) => a.to_lowercase().starts_with(&b.to_lowercase()),
            _ => false,
        },
        "ends_with" => match (actual.and_then(|v| v.as_str()), rule.value.as_str()) {
            (Some(a), Some(b)) => a.to_lowercase().ends_with(&b.to_lowercase()),
            _ => false,
        },
        "gt" | "greater_than" => compare_numbers(actual, &rule.value, |a, b| a > b),
        "lt" | "less_than" => compare_numbers(actual, &rule.value, |a, b| a < b),
        _ => false,
    }
}

fn compare_numbers(
    actual: Option<&serde_json::Value>,
    expected: &serde_json::Value,
    cmp: impl Fn(f64, f64) -> bool,
) -> bool {
    let a = actual.and_then(|v| v.as_f64());
    let b = expected.as_f64();
    match (a, b) {
        (Some(a), Some(b)) => cmp(a, b),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Eksekusi workflow
// ---------------------------------------------------------------------------

/// Jalankan semua workflow aktif untuk satu bot + trigger.
/// Dipanggil dari pemrosesan update (webhook/polling) dan scheduler.
pub async fn run_workflows(
    state: &AppState,
    bot_id: &str,
    trigger: &str,
    context: serde_json::Value,
) {
    let workflows = sqlx::query_as::<_, AutomationWorkflow>(
        "SELECT * FROM automation_workflows \
         WHERE bot_id = ? AND trigger_type = ? AND is_enabled = 1",
    )
    .bind(bot_id)
    .bind(trigger)
    .fetch_all(&state.pool)
    .await;
    let workflows = match workflows {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Gagal memuat workflow automation: {e}");
            return;
        }
    };
    for workflow in workflows {
        run_single_workflow(state, &workflow, trigger, &context).await;
    }
}

async fn run_single_workflow(
    state: &AppState,
    workflow: &AutomationWorkflow,
    trigger: &str,
    context: &serde_json::Value,
) {
    let started = Instant::now();

    // Loop guard: tolak bila sudah 10 eksekusi dalam 1 menit.
    if !state.automation_guard.allow(&workflow.id) {
        tracing::warn!(
            "Workflow {} diblokir loop guard (>10 eksekusi/menit)",
            workflow.id
        );
        record_execution(
            state,
            workflow,
            trigger,
            "blocked",
            0,
            Some("Loop guard: melebihi 10 eksekusi per menit".to_string()),
            started.elapsed().as_millis() as i64,
        )
        .await;
        return;
    }

    // Trigger keyword: cocokkan kata kunci dari trigger_config bila ada.
    if workflow.trigger_type == "keyword" {
        if let Some(cfg) = workflow.trigger_config.parse::<serde_json::Value>().ok().as_ref() {
            if let Some(keywords) = cfg.get("keywords").and_then(|k| k.as_array()) {
                let text = context
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                let matched = keywords.iter().any(|k| {
                    k.as_str()
                        .map(|kw| text.contains(&kw.to_lowercase()))
                        .unwrap_or(false)
                });
                if !matched {
                    return; // Bukan keyword yang ditarget: diam, bukan error.
                }
            }
        }
    }

    // Evaluasi kondisi.
    if !evaluate_conditions(&workflow.conditions, context) {
        return;
    }

    // Eksekusi actions (maks 20 langkah).
    let actions: Vec<serde_json::Value> =
        serde_json::from_str(&workflow.actions).unwrap_or_default();
    let mut steps = 0usize;
    let mut error: Option<String> = None;
    for action in actions.iter().take(MAX_STEPS_PER_EXECUTION) {
        steps += 1;
        // Builder frontend menyimpan delay per aksi sebagai
        // `delay_seconds`; hormati di sini (maks 30 detik, sama seperti
        // aksi delay).
        if let Some(secs) = action.get("delay_seconds").and_then(|v| v.as_u64()) {
            if secs > 0 {
                tokio::time::sleep(Duration::from_secs(secs.min(30))).await;
            }
        }
        if let Err(e) = execute_action(state, workflow, action, context).await {
            error = Some(e);
            break;
        }
    }

    let status = if error.is_some() { "failed" } else { "success" };
    record_execution(
        state,
        workflow,
        trigger,
        status,
        steps as i64,
        error,
        started.elapsed().as_millis() as i64,
    )
    .await;

    // Update counter workflow.
    let now = now_rfc3339();
    let _ = sqlx::query(
        "UPDATE automation_workflows SET execution_count = execution_count + 1, \
         last_executed_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(&workflow.id)
    .execute(&state.pool)
    .await;
}

async fn record_execution(
    state: &AppState,
    workflow: &AutomationWorkflow,
    trigger: &str,
    status: &str,
    steps: i64,
    error: Option<String>,
    duration_ms: i64,
) {
    let _ = sqlx::query(
        "INSERT INTO automation_executions \
         (id, workflow_id, bot_id, trigger_event, status, steps_executed, error_message, duration_ms, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(&workflow.id)
    .bind(&workflow.bot_id)
    .bind(trigger)
    .bind(status)
    .bind(steps)
    // Error eksekusi diredaksi sebelum disimpan (tidak boleh ada token).
    .bind(error.map(|e| crate::security::redact::redact_secrets(&e)))
    .bind(duration_ms)
    .bind(now_rfc3339())
    .execute(&state.pool)
    .await;

    // --- Outgoing webhook: automation.executed (non-blocking, spawn) ---
    dispatch_outgoing_event(
        state,
        "automation.executed",
        Some(&workflow.bot_id),
        serde_json::json!({
            "workflow_id": workflow.id,
            "workflow_name": workflow.name,
            "trigger": trigger,
            "status": status,
            "steps_executed": steps,
            "duration_ms": duration_ms,
        }),
    )
    .await;
}

/// Ganti placeholder `{{field}}` di template dengan nilai dari konteks.
pub fn render_template(template: &str, context: &serde_json::Value) -> String {
    let mut out = template.to_string();
    if let Some(obj) = context.as_object() {
        for (key, value) in obj {
            let placeholder = format!("{{{{{key}}}}}");
            let replacement = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out = out.replace(&placeholder, &replacement);
        }
    }
    out
}

/// Teks aksi: field `text` (utama) atau alias `message`.
pub fn action_text(action: &serde_json::Value) -> Option<&str> {
    action
        .get("text")
        .and_then(|t| t.as_str())
        .or_else(|| action.get("message").and_then(|t| t.as_str()))
}

/// Parse `admin_chat_id` aksi notify_admin: angka JSON atau string angka.
/// Kosong/null = tidak diisi (None). Nilai lain yang tidak valid = Err.
pub fn parse_admin_chat_id(action: &serde_json::Value) -> Result<Option<i64>, String> {
    match action.get("admin_chat_id") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => match n.as_i64() {
            // 0 = sentinel form kosong, dianggap tidak diisi.
            Some(0) => Ok(None),
            Some(v) => Ok(Some(v)),
            None => Err("notify_admin: admin_chat_id harus berupa angka".to_string()),
        },
        Some(serde_json::Value::String(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() || trimmed == "0" {
                return Ok(None);
            }
            trimmed
                .parse::<i64>()
                .map(Some)
                .map_err(|_| {
                    "notify_admin: admin_chat_id harus berupa angka (ID numerik Telegram)".to_string()
                })
        }
        Some(_) => Err("notify_admin: admin_chat_id harus berupa angka".to_string()),
    }
}

async fn execute_action(
    state: &AppState,
    workflow: &AutomationWorkflow,
    action: &serde_json::Value,
    context: &serde_json::Value,
) -> Result<(), String> {
    let action_type = action
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let chat_id = action
        .get("chat_id")
        .and_then(|v| v.as_i64())
        .or_else(|| context.get("chat_id").and_then(|v| v.as_i64()));
    let user_id = context.get("user_id").and_then(|v| v.as_i64());

    match action_type.as_str() {
        "send_message" => {
            let chat_id = chat_id.ok_or_else(|| "send_message: chat_id tidak ada".to_string())?;
            let text = action
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let text = render_template(&text, context);
            let token = get_bot_token(state, &workflow.bot_id)
                .await
                .map_err(|e| e.to_string())?;
            state
                .telegram
                .send_message(
                    &token,
                    &SendMessageParams {
                        chat_id,
                        text,
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        "send_template" => {
            let chat_id = chat_id.ok_or_else(|| "send_template: chat_id tidak ada".to_string())?;
            let template_id = action
                .get("template_id")
                .and_then(|t| t.as_str())
                .ok_or_else(|| "send_template: template_id tidak ada".to_string())?;
            let template = sqlx::query_as::<_, MessageTemplate>(
                "SELECT * FROM message_templates WHERE id = ?",
            )
            .bind(template_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("template {template_id} tidak ditemukan"))?;
            let text = render_template(&template.content, context);
            let token = get_bot_token(state, &workflow.bot_id)
                .await
                .map_err(|e| e.to_string())?;
            state
                .telegram
                .send_message(
                    &token,
                    &SendMessageParams {
                        chat_id,
                        text,
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        "add_label" => {
            let user_id = user_id.ok_or_else(|| "add_label: user_id tidak ada".to_string())?;
            let label = action
                .get("label")
                .and_then(|t| t.as_str())
                .ok_or_else(|| "add_label: label tidak ada".to_string())?;
            let row: Option<(String,)> = sqlx::query_as(
                "SELECT labels FROM bot_users WHERE bot_id = ? AND telegram_user_id = ?",
            )
            .bind(&workflow.bot_id)
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|e| e.to_string())?;
            if let Some((labels_json,)) = row {
                let mut labels: Vec<String> =
                    serde_json::from_str(&labels_json).unwrap_or_default();
                if !labels.iter().any(|l| l == label) {
                    labels.push(label.to_string());
                }
                let _ = sqlx::query(
                    "UPDATE bot_users SET labels = ?, updated_at = ? \
                     WHERE bot_id = ? AND telegram_user_id = ?",
                )
                .bind(serde_json::to_string(&labels).unwrap_or_else(|_| "[]".to_string()))
                .bind(now_rfc3339())
                .bind(&workflow.bot_id)
                .bind(user_id)
                .execute(&state.pool)
                .await;
            }
            Ok(())
        }
        "save_note" => {
            let user_id = user_id.ok_or_else(|| "save_note: user_id tidak ada".to_string())?;
            let note = action
                .get("note")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let note = render_template(&note, context);
            let _ = sqlx::query(
                "UPDATE bot_users SET notes = ?, updated_at = ? \
                 WHERE bot_id = ? AND telegram_user_id = ?",
            )
            .bind(note)
            .bind(now_rfc3339())
            .bind(&workflow.bot_id)
            .bind(user_id)
            .execute(&state.pool)
            .await;
            Ok(())
        }
        "save_activity" => {
            let raw_text = action_text(action)
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| "save_activity: teks tidak boleh kosong".to_string())?;
            let text = render_template(raw_text, context);
            if text.trim().is_empty() {
                return Err("save_activity: teks tidak boleh kosong".to_string());
            }
            let label = action
                .get("label")
                .and_then(|t| t.as_str())
                .map(|s| render_template(s, context))
                .filter(|s| !s.trim().is_empty());
            let level = match action.get("level").and_then(|t| t.as_str()) {
                Some("warning") => "warning",
                Some("error") => "error",
                _ => "info",
            };
            // Kaitkan ke user terkait bila ada, selain itu ke chat.
            let (entity_type, entity_id) = if let Some(uid) = user_id {
                (Some("bot_user"), Some(uid.to_string()))
            } else if let Some(cid) = chat_id {
                (Some("bot_chat"), Some(cid.to_string()))
            } else {
                (None, None)
            };
            // log_activity meredaksi details sebelum disimpan.
            log_activity(
                state,
                Some(&workflow.bot_id),
                "automation_activity",
                entity_type,
                entity_id.as_deref(),
                serde_json::json!({
                    "message": text,
                    "text": text,
                    "label": label,
                    "workflow_id": workflow.id,
                    "workflow_name": workflow.name,
                    "chat_id": chat_id,
                    "user_id": user_id,
                }),
                level,
            )
            .await;
            Ok(())
        }
        "notify_admin" => {
            let raw_text = action_text(action)
                .filter(|t| !t.trim().is_empty())
                .ok_or_else(|| "notify_admin: teks tidak boleh kosong".to_string())?;
            let text = render_template(raw_text, context);
            if text.trim().is_empty() {
                return Err("notify_admin: teks tidak boleh kosong".to_string());
            }
            let admin_chat_id = parse_admin_chat_id(action)?;
            let title = action
                .get("title")
                .and_then(|t| t.as_str())
                .map(|s| render_template(s, context))
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("Automation: {}", workflow.name));
            // Notifikasi in-app selalu dibuat (create_notification
            // meredaksi teks/metadata sebelum disimpan).
            create_notification(
                state,
                "automation_admin",
                &title,
                &text,
                serde_json::json!({
                    "bot_id": workflow.bot_id,
                    "workflow_id": workflow.id,
                    "workflow_name": workflow.name,
                    "chat_id": chat_id,
                    "user_id": user_id,
                    "admin_chat_id": admin_chat_id,
                }),
            )
            .await;
            // Bila admin_chat_id diisi, kirim juga pesan Telegram ke chat
            // itu memakai bot workflow.
            if let Some(admin_chat_id) = admin_chat_id {
                let token = get_bot_token(state, &workflow.bot_id)
                    .await
                    .map_err(|e| e.to_string())?;
                state
                    .telegram
                    .send_message(
                        &token,
                        &SendMessageParams {
                            chat_id: admin_chat_id,
                            text,
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        "delay" => {
            let seconds = action
                .get("seconds")
                .and_then(|v| v.as_u64())
                .unwrap_or(1)
                .min(30); // batasi delay maks 30 detik per langkah
            tokio::time::sleep(Duration::from_secs(seconds)).await;
            Ok(())
        }
        other => Err(format!("action tidak dikenal: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn conditions_empty_matches() {
        let ctx = json!({"text": "halo"});
        assert!(evaluate_conditions("", &ctx));
        assert!(evaluate_conditions("[]", &ctx));
        assert!(evaluate_conditions("{}", &ctx));
    }

    #[test]
    fn conditions_and_logic() {
        let ctx = json!({"text": "halo dunia", "chat_id": 123});
        let cond = r#"{"logic": "AND", "rules": [
            {"field": "text", "operator": "contains", "value": "halo"},
            {"field": "chat_id", "operator": "eq", "value": 123}
        ]}"#;
        assert!(evaluate_conditions(cond, &ctx));
        let cond_no = r#"{"logic": "AND", "rules": [
            {"field": "text", "operator": "contains", "value": "halo"},
            {"field": "chat_id", "operator": "eq", "value": 999}
        ]}"#;
        assert!(!evaluate_conditions(cond_no, &ctx));
    }

    #[test]
    fn conditions_or_logic_and_array_form() {
        let ctx = json!({"text": "selamat pagi"});
        let cond = r#"{"logic": "OR", "rules": [
            {"field": "text", "operator": "contains", "value": "malam"},
            {"field": "text", "operator": "starts_with", "value": "selamat"}
        ]}"#;
        assert!(evaluate_conditions(cond, &ctx));
        // Bentuk array langsung (AND).
        let cond_arr = r#"[{"field": "text", "operator": "contains", "value": "pagi"}]"#;
        assert!(evaluate_conditions(cond_arr, &ctx));
        let cond_arr_no = r#"[{"field": "text", "operator": "contains", "value": "siang"}]"#;
        assert!(!evaluate_conditions(cond_arr_no, &ctx));
    }

    #[test]
    fn conditions_numeric_and_invalid_json() {
        let ctx = json!({"user_id": 50});
        assert!(evaluate_conditions(
            r#"[{"field": "user_id", "operator": "gt", "value": 10}]"#,
            &ctx
        ));
        assert!(!evaluate_conditions(
            r#"[{"field": "user_id", "operator": "lt", "value": 10}]"#,
            &ctx
        ));
        // JSON tidak valid => fail-closed (tidak cocok).
        assert!(!evaluate_conditions("{bukan json", &ctx));
    }

    #[test]
    fn loop_guard_blocks_11th_execution_in_one_minute() {
        let guard = AutomationGuard::new();
        for i in 1..=10 {
            assert!(guard.allow("wf-1"), "eksekusi ke-{i} harus diizinkan");
        }
        assert!(!guard.allow("wf-1"), "eksekusi ke-11 harus ditolak");
        // Workflow lain tidak terpengaruh.
        assert!(guard.allow("wf-2"));
    }

    #[test]
    fn template_rendering() {
        let ctx = json!({"first_name": "Astr", "text": "halo"});
        assert_eq!(
            render_template("Halo {{first_name}}, kamu bilang: {{text}}", &ctx),
            "Halo Astr, kamu bilang: halo"
        );
    }

    #[test]
    fn parse_admin_chat_id_accepts_number_and_numeric_string() {
        assert_eq!(
            parse_admin_chat_id(&json!({"admin_chat_id": 999})).unwrap(),
            Some(999)
        );
        assert_eq!(
            parse_admin_chat_id(&json!({"admin_chat_id": "-100123"})).unwrap(),
            Some(-100123)
        );
        assert_eq!(parse_admin_chat_id(&json!({})).unwrap(), None);
        assert_eq!(
            parse_admin_chat_id(&json!({"admin_chat_id": ""})).unwrap(),
            None
        );
        assert!(parse_admin_chat_id(&json!({"admin_chat_id": "abc"})).is_err());
        // 0 = sentinel form kosong, dianggap tidak diisi.
        assert_eq!(parse_admin_chat_id(&json!({"admin_chat_id": 0})).unwrap(), None);
    }

    async fn test_state() -> (AppState, std::sync::mpsc::Receiver<String>) {
        use crate::bot::telegram::test_support::fake_telegram_server;
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        // Server Telegram palsu: semua sendMessage berhasil.
        let (base, rx) = fake_telegram_server(
            r#"{"ok":true,"result":{"message_id":1,"from":{"id":111111111,"is_bot":true,"first_name":"Test"},"chat":{"id":777,"type":"private"},"date":1793412000,"text":"ok"}}"#,
        );
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
        let crypto =
            std::sync::Arc::new(crate::security::Crypto::from_key_bytes([9u8; 32]));
        let token = format!("111111111:{}", "FAKE".repeat(9));
        let encrypted = crypto.encrypt(&token).unwrap();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO bots (id, name, telegram_id, token_encrypted, token_masked, status, created_at, updated_at) \
             VALUES ('bot-1', 'Bot Test', 111111111, ?, '****', 'connected', ?, ?)",
        )
        .bind(&encrypted)
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        let telegram = crate::bot::telegram::TelegramClient::with_base_url(base).unwrap();
        (AppState::new(pool, config, crypto, telegram), rx)
    }

    async fn seed_workflow(
        state: &AppState,
        id: &str,
        conditions: serde_json::Value,
        actions: serde_json::Value,
    ) {
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO automation_workflows (id, bot_id, name, trigger_type, trigger_config, conditions, actions, created_at, updated_at) \
             VALUES (?, 'bot-1', 'Workflow Test', 'message_received', '{}', ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(conditions.to_string())
        .bind(actions.to_string())
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn save_activity_writes_activity_log() {
        let (state, _rx) = test_state().await;
        seed_workflow(
            &state,
            "wf-save",
            json!([]),
            json!([{"type": "save_activity", "text": "User {{first_name}} bertanya", "label": "tanya"}]),
        )
        .await;
        run_workflows(
            &state,
            "bot-1",
            "message_received",
            json!({"text": "halo", "chat_id": 777, "user_id": 777, "first_name": "Budi"}),
        )
        .await;
        let (entity_type, entity_id, details): (String, String, String) = sqlx::query_as(
            "SELECT entity_type, entity_id, details FROM activity_logs WHERE action = 'automation_activity' LIMIT 1",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert_eq!(entity_type, "bot_user");
        assert_eq!(entity_id, "777");
        assert!(details.contains("User Budi bertanya"));
        assert!(details.contains("tanya"));
    }

    #[tokio::test]
    async fn notify_admin_creates_notification_and_sends_to_admin_chat() {
        let (state, rx) = test_state().await;
        seed_workflow(
            &state,
            "wf-notify",
            json!([]),
            json!([{"type": "notify_admin", "text": "Ada pesan dari {{first_name}}", "admin_chat_id": 999}]),
        )
        .await;
        run_workflows(
            &state,
            "bot-1",
            "message_received",
            json!({"text": "halo", "chat_id": 777, "user_id": 777, "first_name": "Budi"}),
        )
        .await;
        let (title, message): (String, String) = sqlx::query_as(
            "SELECT title, message FROM notifications WHERE type = 'automation_admin' LIMIT 1",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert!(title.contains("Workflow Test"));
        assert_eq!(message, "Ada pesan dari Budi");
        // Pesan Telegram terkirim ke admin_chat_id memakai bot workflow.
        let req = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(req.contains("sendMessage"));
        assert!(req.contains("999"));
        assert!(req.contains("Ada pesan dari Budi"));
    }

    #[tokio::test]
    async fn astr_example_workflow_runs_end_to_end_via_process_update() {
        let (state, rx) = test_state().await;
        // Contoh Astr: trigger pesan masuk + condition teks mengandung
        // kata kunci + send_message + save_activity + notify_admin.
        seed_workflow(
            &state,
            "wf-astr",
            json!({"logic": "AND", "rules": [{"field": "text", "operator": "contains", "value": "harga"}]}),
            json!([
                {"type": "send_message", "text": "Halo {{first_name}}, harga produk kami terjangkau."},
                {"type": "save_activity", "text": "User bertanya harga: {{text}}", "label": "harga"},
                {"type": "notify_admin", "text": "Pertanyaan harga dari {{first_name}}", "admin_chat_id": 999}
            ]),
        )
        .await;
        let update: crate::bot::telegram::Update = serde_json::from_value(json!({
            "update_id": 1,
            "message": {
                "message_id": 10,
                "from": {"id": 777, "is_bot": false, "first_name": "Budi", "username": "budi"},
                "chat": {"id": 777, "type": "private", "first_name": "Budi", "username": "budi"},
                "date": 1793412000,
                "text": "berapa harga produk ini?"
            }
        }))
        .unwrap();
        crate::services::process_update(&state, "bot-1", &update).await;

        // Eksekusi tercatat sukses dengan 3 langkah.
        let (status, steps): (String, i64) = sqlx::query_as(
            "SELECT status, steps_executed FROM automation_executions WHERE workflow_id = 'wf-astr' LIMIT 1",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert_eq!(status, "success");
        assert_eq!(steps, 3);
        // save_activity menulis activity log untuk user terkait.
        let details: String = sqlx::query_scalar(
            "SELECT details FROM activity_logs WHERE action = 'automation_activity' AND entity_id = '777' LIMIT 1",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert!(details.contains("berapa harga produk ini?"));
        // notify_admin membuat notifikasi in-app.
        let message: String = sqlx::query_scalar(
            "SELECT message FROM notifications WHERE type = 'automation_admin' LIMIT 1",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert_eq!(message, "Pertanyaan harga dari Budi");
        // Dua pesan Telegram: balasan ke user + notifikasi ke admin.
        let req1 = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let req2 = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let all = format!("{req1}{req2}");
        assert!(all.contains("777"));
        assert!(all.contains("999"));
    }
}
