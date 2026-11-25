//! Automation engine.
//!
//! Trigger: new_user, keyword, command, message_received, scheduled,
//! user_joined_group.
//! Condition (JSON): `{ "logic": "AND"|"OR", "rules": [{ "field", "operator",
//! "value" }] }` atau array rules langsung (dianggap AND).
//! Operator: eq/equals, ne/not_equals, contains, not_contains, starts_with,
//! ends_with, gt/greater_than, lt/less_than.
//! Action: send_message, send_template, add_label, save_note, delay.
//!
//! Loop guard: maks 10 eksekusi per workflow per menit, maks 20 langkah per
//! eksekusi, pesan dari bot sendiri diabaikan (dicek di `process_update`).

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::bot::get_bot_token;
use crate::bot::telegram::SendMessageParams;
use crate::models::{new_id, now_rfc3339, AutomationWorkflow, MessageTemplate};
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
    .bind(error)
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
}
