//! AI Smart Reply (v1.2).
//!
//! - Provider: `openai_compatible` (endpoint OpenAI-compatible apa pun),
//!   `anthropic`, `gemini`. API key disimpan terenkripsi (AES-256-GCM,
//!   helper yang sama dengan token bot) dan TIDAK PERNAH dikembalikan via API.
//! - Konstruksi request & parsing respons dipisah sebagai fungsi murni agar
//!   bisa diuji tanpa network / tanpa key sungguhan.
//! - Guardrail (lihat [`should_ai_reply`]): hanya chat privat kecuali
//!   `ai_allow_groups`, abaikan pesan dari bot, rate limit 10 balasan
//!   per (bot, user) per jam, maks 1 balasan per pesan masuk, anti-loop.
//! - Bila provider error: JANGAN kirim pesan error ke user; hanya catat
//!   ke activity log.

use std::time::Duration;

use chrono::Utc;
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};
use crate::models::{now_rfc3339, BotConfiguration};
use crate::services::log_activity;
use crate::state::AppState;

pub const AI_MAX_REPLIES_PER_HOUR: i64 = 10;
pub const AI_CHAT_TIMEOUT_SECS: u64 = 30;

/// Bucket jam UTC format `YYYY-MM-DD-HH`.
pub fn hour_bucket(now: chrono::DateTime<Utc>) -> String {
    now.format("%Y-%m-%d-%H").to_string()
}

/// Konfigurasi AI yang dibutuhkan untuk satu balasan.
#[derive(Debug, Clone)]
pub struct AiReplyConfig {
    pub provider_type: String,
    pub base_url: Option<String>,
    pub model: String,
    /// API key plaintext — hanya hidup di memori selama satu panggilan.
    pub api_key: Option<String>,
    pub system_prompt: Option<String>,
    pub max_tokens: i64,
    pub temperature: f64,
}

// ---------------------------------------------------------------------------
// Konstruksi request (fungsi murni — diuji unit tanpa network)
// ---------------------------------------------------------------------------

/// URL + body JSON untuk chat completion OpenAI-compatible.
/// Mengembalikan (url, body). Header Authorization ditambahkan pemanggil.
pub fn build_openai_chat_request(
    base_url: &str,
    model: &str,
    system_prompt: Option<&str>,
    user_text: &str,
    max_tokens: i64,
    temperature: f64,
) -> (String, serde_json::Value) {
    let base = base_url.trim_end_matches('/');
    let mut messages = Vec::new();
    if let Some(sys) = system_prompt.filter(|s| !s.trim().is_empty()) {
        messages.push(serde_json::json!({"role": "system", "content": sys}));
    }
    messages.push(serde_json::json!({"role": "user", "content": user_text}));
    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": temperature,
    });
    (format!("{base}/chat/completions"), body)
}

/// URL + body JSON untuk Anthropic Messages API.
pub fn build_anthropic_chat_request(
    model: &str,
    system_prompt: Option<&str>,
    user_text: &str,
    max_tokens: i64,
    temperature: f64,
) -> (String, serde_json::Value) {
    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "temperature": temperature,
        "messages": [{"role": "user", "content": user_text}],
    });
    if let Some(sys) = system_prompt.filter(|s| !s.trim().is_empty()) {
        body["system"] = serde_json::Value::String(sys.to_string());
    }
    ("https://api.anthropic.com/v1/messages".to_string(), body)
}

/// URL + body JSON untuk Gemini generateContent.
pub fn build_gemini_chat_request(
    model: &str,
    system_prompt: Option<&str>,
    user_text: &str,
    max_tokens: i64,
    temperature: f64,
) -> (String, serde_json::Value) {
    let prompt = match system_prompt.filter(|s| !s.trim().is_empty()) {
        Some(sys) => format!("{sys}\n\n{user_text}"),
        None => user_text.to_string(),
    };
    let body = serde_json::json!({
        "contents": [{"parts": [{"text": prompt}]}],
        "generationConfig": {"maxOutputTokens": max_tokens, "temperature": temperature},
    });
    (
        format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"),
        body,
    )
}

/// URL untuk tes koneksi per provider (tanpa key di URL kecuali Gemini,
/// yang memakai `?key=` sesuai API Google — key tidak pernah di-log).
pub fn build_test_url(provider_type: &str, base_url: Option<&str>) -> AppResult<String> {
    match provider_type {
        "openai_compatible" => {
            let base = base_url.unwrap_or("").trim_end_matches('/');
            if base.is_empty() {
                return Err(AppError::Validation(
                    "openai_compatible membutuhkan base_url".to_string(),
                ));
            }
            Ok(format!("{base}/models"))
        }
        "anthropic" => Ok("https://api.anthropic.com/v1/models".to_string()),
        "gemini" => Ok("https://generativelanguage.googleapis.com/v1beta/models".to_string()),
        other => Err(AppError::Validation(format!(
            "provider_type tidak dikenal: '{other}'"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Parsing respons (fungsi murni — diuji unit dengan JSON contoh)
// ---------------------------------------------------------------------------

pub fn parse_openai_chat_response(body: &serde_json::Value) -> Option<String> {
    body.get("choices")?
        .as_array()?
        .first()?
        .get("message")?
        .get("content")?
        .as_str()
        .map(|s| s.to_string())
}

pub fn parse_anthropic_chat_response(body: &serde_json::Value) -> Option<String> {
    let text: String = body
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|block| {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                block.get("text").and_then(|t| t.as_str())
            } else {
                None
            }
        })
        .collect();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub fn parse_gemini_chat_response(body: &serde_json::Value) -> Option<String> {
    let parts = body
        .get("candidates")?
        .as_array()?
        .first()?
        .get("content")?
        .get("parts")?
        .as_array()?;
    let text: String = parts
        .iter()
        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
        .collect();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Hitung jumlah model dari respons list-models per provider.
pub fn count_models(provider_type: &str, body: &serde_json::Value) -> Option<usize> {
    match provider_type {
        "openai_compatible" | "anthropic" => {
            body.get("data")?.as_array().map(|a| a.len())
        }
        "gemini" => body.get("models")?.as_array().map(|a| a.len()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Guardrail (fungsi murni — diuji unit)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct AiGuardInput {
    pub chat_type: String,
    pub allow_groups: bool,
    pub from_is_bot: bool,
    /// Jumlah balasan AI untuk (bot, user) pada jam berjalan.
    pub usage_count_this_hour: i64,
    /// AI sudah pernah membalas pesan masuk ini (reply_to_message_id sama)
    /// — mis. update di-deliver ulang / pesan diedit.
    pub already_replied_to_this_message: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiBlockReason {
    GroupNotAllowed,
    FromBot,
    RateLimited,
    AlreadyReplied,
}

pub fn should_ai_reply(input: &AiGuardInput) -> Result<(), AiBlockReason> {
    if input.from_is_bot {
        return Err(AiBlockReason::FromBot);
    }
    if input.chat_type != "private" && !input.allow_groups {
        return Err(AiBlockReason::GroupNotAllowed);
    }
    if input.already_replied_to_this_message {
        return Err(AiBlockReason::AlreadyReplied);
    }
    if input.usage_count_this_hour >= AI_MAX_REPLIES_PER_HOUR {
        return Err(AiBlockReason::RateLimited);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Rate limit di DB (atomik: satu statement INSERT ... ON CONFLICT ... RETURNING)
// ---------------------------------------------------------------------------

/// Cek + increment pemakaian AI untuk (bot, user) pada jam berjalan.
/// Mengembalikan `Ok(true)` bila masih di bawah batas (kuota terpakai),
/// `Ok(false)` bila sudah mencapai 10/jam (kuota TIDAK bertambah lagi
/// setelah batas? — counter tetap naik agar terlihat di DB, tapi balasan
/// ditolak; lihat `should_ai_reply`).
pub async fn try_consume_ai_reply(
    pool: &SqlitePool,
    bot_id: &str,
    user_id: i64,
) -> AppResult<bool> {
    let bucket = hour_bucket(Utc::now());
    let count: i64 = sqlx::query_scalar(
        "INSERT INTO ai_usage (bot_id, user_id, hour_bucket, count) VALUES (?, ?, ?, 1) \
         ON CONFLICT(bot_id, user_id, hour_bucket) DO UPDATE SET count = count + 1 \
         RETURNING count",
    )
    .bind(bot_id)
    .bind(user_id.to_string())
    .bind(&bucket)
    .fetch_one(pool)
    .await?;
    Ok(count <= AI_MAX_REPLIES_PER_HOUR)
}

pub async fn ai_usage_count(pool: &SqlitePool, bot_id: &str, user_id: i64) -> i64 {
    let bucket = hour_bucket(Utc::now());
    sqlx::query_scalar::<_, i64>(
        "SELECT count FROM ai_usage WHERE bot_id = ? AND user_id = ? AND hour_bucket = ?",
    )
    .bind(bot_id)
    .bind(user_id.to_string())
    .bind(&bucket)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Panggilan chat ke provider
// ---------------------------------------------------------------------------

pub async fn chat_reply(cfg: &AiReplyConfig, user_text: &str) -> AppResult<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(AI_CHAT_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::Internal(format!("gagal membuat HTTP client AI: {e}")))?;
    let (url, body, reply) = match cfg.provider_type.as_str() {
        "openai_compatible" => {
            let base = cfg.base_url.clone().unwrap_or_default();
            let (url, body) = build_openai_chat_request(
                &base,
                &cfg.model,
                cfg.system_prompt.as_deref(),
                user_text,
                cfg.max_tokens,
                cfg.temperature,
            );
            let key = cfg.api_key.clone().unwrap_or_default();
            let resp = client
                .post(&url)
                .bearer_auth(key)
                .json(&body)
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("panggilan AI gagal: {e}")))?;
            let status = resp.status();
            let json: serde_json::Value = resp.json().await.map_err(|e| {
                AppError::Internal(format!("respons AI tidak valid: {e}"))
            })?;
            if !status.is_success() {
                return Err(AppError::Internal(format!(
                    "provider AI error {}: {}",
                    status,
                    json.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown error")
                )));
            }
            (url, body, parse_openai_chat_response(&json))
        }
        "anthropic" => {
            let (url, body) = build_anthropic_chat_request(
                &cfg.model,
                cfg.system_prompt.as_deref(),
                user_text,
                cfg.max_tokens,
                cfg.temperature,
            );
            let key = cfg.api_key.clone().unwrap_or_default();
            let resp = client
                .post(&url)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("panggilan AI gagal: {e}")))?;
            let status = resp.status();
            let json: serde_json::Value = resp.json().await.map_err(|e| {
                AppError::Internal(format!("respons AI tidak valid: {e}"))
            })?;
            if !status.is_success() {
                return Err(AppError::Internal(format!(
                    "provider AI error {}: {}",
                    status,
                    json.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown error")
                )));
            }
            (url, body, parse_anthropic_chat_response(&json))
        }
        "gemini" => {
            let (mut url, body) = build_gemini_chat_request(
                &cfg.model,
                cfg.system_prompt.as_deref(),
                user_text,
                cfg.max_tokens,
                cfg.temperature,
            );
            let key = cfg.api_key.clone().unwrap_or_default();
            url = format!("{url}?key={key}");
            let resp = client
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| AppError::Internal(format!("panggilan AI gagal: {e}")))?;
            let status = resp.status();
            let json: serde_json::Value = resp.json().await.map_err(|e| {
                AppError::Internal(format!("respons AI tidak valid: {e}"))
            })?;
            if !status.is_success() {
                return Err(AppError::Internal(format!(
                    "provider AI error {}: {}",
                    status,
                    json.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown error")
                )));
            }
            (url, body, parse_gemini_chat_response(&json))
        }
        other => {
            return Err(AppError::Validation(format!(
                "provider_type tidak dikenal: '{other}'"
            )))
        }
    };
    let _ = (url, body);
    reply
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| AppError::Internal("provider AI mengembalikan jawaban kosong".to_string()))
}

// ---------------------------------------------------------------------------
// Alur lengkap dari update_processor
// ---------------------------------------------------------------------------

/// Apakah automation (command kustom / keyword workflow) sudah menangani
/// pesan ini? Dipakai agar AI hanya menjawab bila tidak ada yang cocok.
/// Ini pendekatan best-effort berbasis DB (command aktif + keyword workflow
/// aktif); `run_workflows` sendiri tidak mengembalikan status cocok.
pub async fn automation_handled(state: &AppState, bot_id: &str, text: &str) -> bool {
    if text.starts_with('/') {
        let first = text.split_whitespace().next().unwrap_or("");
        let name = first.trim_start_matches('/').split('@').next().unwrap_or("");
        if !name.is_empty() {
            let hit: Option<(String,)> = sqlx::query_as(
                "SELECT id FROM bot_commands WHERE bot_id = ? AND name = ? AND is_enabled = 1",
            )
            .bind(bot_id)
            .bind(name)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
            if hit.is_some() {
                return true;
            }
        }
    }
    // Keyword workflow aktif: cocokkan substring (case-insensitive).
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT trigger_config FROM automation_workflows \
         WHERE bot_id = ? AND trigger_type = 'keyword' AND is_enabled = 1",
    )
    .bind(bot_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let lower = text.to_lowercase();
    for (cfg, ) in rows {
        if let Ok(v) = cfg.parse::<serde_json::Value>() {
            if let Some(kws) = v.get("keywords").and_then(|k| k.as_array()) {
                for kw in kws {
                    if let Some(k) = kw.as_str() {
                        if !k.is_empty() && lower.contains(&k.to_lowercase()) {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Coba balas pesan masuk dengan AI. Mengembalikan `true` bila AI membalas.
/// Dipanggil update_processor setelah command/automation. Semua kegagalan
/// provider dicatat ke activity log saja — TIDAK ada pesan error ke user.
pub async fn maybe_ai_reply(
    state: &AppState,
    bot_id: &str,
    chat_id: i64,
    chat_type: &str,
    user_id: Option<i64>,
    telegram_message_id: i64,
    text: &str,
) -> bool {
    let Some(user_id) = user_id else { return false };
    if text.trim().is_empty() {
        return false;
    }
    let cfg = match sqlx::query_as::<_, BotConfiguration>(
        "SELECT * FROM bot_configurations WHERE bot_id = ?",
    )
    .bind(bot_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    {
        Some(c) => c,
        None => return false,
    };
    if cfg.ai_enabled != 1 {
        return false;
    }
    let Some(provider_id) = cfg.ai_provider_id.clone() else {
        return false;
    };
    // Anti-loop: AI sudah membalas pesan ini (update di-deliver ulang / edit).
    let already_replied: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM messages WHERE bot_id = ? AND chat_id = ? \
         AND direction = 'outbound' AND reply_to_message_id = ? LIMIT 1",
    )
    .bind(bot_id)
    .bind(chat_id)
    .bind(telegram_message_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let usage = ai_usage_count(&state.pool, bot_id, user_id).await;
    let guard = AiGuardInput {
        chat_type: chat_type.to_string(),
        allow_groups: cfg.ai_allow_groups == 1,
        from_is_bot: false, // pengirim bot sudah disaring update_processor
        usage_count_this_hour: usage,
        already_replied_to_this_message: already_replied.is_some(),
    };
    if let Err(reason) = should_ai_reply(&guard) {
        tracing::debug!("AI Smart Reply ditolak untuk bot {bot_id}: {reason:?}");
        return false;
    }
    // Rate limit: cek + increment atomik.
    match try_consume_ai_reply(&state.pool, bot_id, user_id).await {
        Ok(true) => {}
        Ok(false) => {
            log_activity(
                state,
                Some(bot_id),
                "ai_reply_rate_limited",
                Some("bot_user"),
                Some(&user_id.to_string()),
                serde_json::json!({"chat_id": chat_id}),
                "warning",
            )
            .await;
            return false;
        }
        Err(e) => {
            tracing::warn!("AI rate limit gagal dicek: {e}");
            return false;
        }
    }
    // Muat provider + dekripsi key.
    let provider = match sqlx::query_as::<_, crate::models::AiProvider>(
        "SELECT * FROM ai_providers WHERE id = ? AND enabled = 1",
    )
    .bind(&provider_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten()
    {
        Some(p) => p,
        None => return false,
    };
    let api_key = match &provider.api_key_encrypted {
        Some(enc) => match state.crypto.decrypt(enc) {
            Ok(k) => Some(k),
            Err(e) => {
                log_activity(
                    state,
                    Some(bot_id),
                    "ai_reply",
                    Some("ai_provider"),
                    Some(&provider.id),
                    serde_json::json!({"error": format!("gagal mendekripsi api_key: {e}")}),
                    "error",
                )
                .await;
                return false;
            }
        },
        None => None,
    };
    let reply_cfg = AiReplyConfig {
        provider_type: provider.provider_type.clone(),
        base_url: provider.base_url.clone(),
        model: provider.model.clone(),
        api_key,
        system_prompt: cfg.ai_system_prompt.clone(),
        max_tokens: cfg.ai_max_tokens,
        temperature: cfg.ai_temperature,
    };
    let answer = match chat_reply(&reply_cfg, text).await {
        Ok(a) => a,
        Err(e) => {
            log_activity(
                state,
                Some(bot_id),
                "ai_reply",
                Some("ai_provider"),
                Some(&provider.id),
                serde_json::json!({"error": e.to_string(), "chat_id": chat_id}),
                "error",
            )
            .await;
            return false;
        }
    };
    // Kirim jawaban via Telegram.
    let token = match crate::bot::get_bot_token(state, bot_id).await {
        Ok(t) => t,
        Err(_) => return false,
    };
    let sent = state
        .telegram
        .send_message(
            &token,
            &crate::bot::telegram::SendMessageParams {
                chat_id,
                text: answer.clone(),
                reply_to_message_id: Some(telegram_message_id),
                ..Default::default()
            },
        )
        .await;
    match sent {
        Ok(tg) => {
            let now = now_rfc3339();
            let _ = sqlx::query(
                "INSERT INTO messages (id, bot_id, chat_id, telegram_message_id, direction, \
                 message_type, content, status, from_user_id, reply_to_message_id, is_read, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, 'outbound', 'text', ?, 'sent', ?, ?, 1, ?, ?)",
            )
            .bind(crate::models::new_id())
            .bind(bot_id)
            .bind(chat_id)
            .bind(tg.message_id)
            .bind(&answer)
            .bind(user_id)
            .bind(telegram_message_id)
            .bind(&now)
            .bind(&now)
            .execute(&state.pool)
            .await;
            log_activity(
                state,
                Some(bot_id),
                "ai_reply",
                Some("ai_provider"),
                Some(&provider.id),
                serde_json::json!({
                    "chat_id": chat_id, "user_id": user_id,
                    "provider": provider.provider_type, "model": provider.model,
                }),
                "info",
            )
            .await;
            true
        }
        Err(e) => {
            log_activity(
                state,
                Some(bot_id),
                "ai_reply",
                Some("ai_provider"),
                Some(&provider.id),
                serde_json::json!({"error": format!("gagal mengirim jawaban AI: {e}")}),
                "error",
            )
            .await;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_request_builds_url_body_headers_shape() {
        let (url, body) = build_openai_chat_request(
            "https://api.example.com/v1/",
            "gpt-test",
            Some("Kamu asisten."),
            "halo",
            500,
            0.7,
        );
        assert_eq!(url, "https://api.example.com/v1/chat/completions");
        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "halo");
        assert_eq!(body["max_tokens"], 500);
        assert_eq!(body["temperature"], 0.7);
        // Tanpa system prompt: hanya 1 message.
        let (_, body) = build_openai_chat_request(
            "https://api.example.com/v1",
            "m",
            None,
            "hai",
            100,
            0.5,
        );
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn anthropic_request_builds_url_and_body() {
        let (url, body) =
            build_anthropic_chat_request("claude-test", Some("Sistem."), "halo", 300, 0.2);
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
        assert_eq!(body["system"], "Sistem.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["max_tokens"], 300);
    }

    #[test]
    fn gemini_request_builds_url_and_body() {
        let (url, body) =
            build_gemini_chat_request("gemini-test", Some("Sistem."), "halo", 200, 0.9);
        assert!(url.contains("gemini-test:generateContent"));
        let text = body["contents"][0]["parts"][0]["text"].as_str().unwrap();
        assert!(text.contains("Sistem."));
        assert!(text.contains("halo"));
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 200);
    }

    #[test]
    fn parse_responses_from_sample_json() {
        let openai: serde_json::Value = serde_json::from_str(
            r#"{"choices":[{"message":{"role":"assistant","content":"Halo juga!"}}]}"#,
        )
        .unwrap();
        assert_eq!(
            parse_openai_chat_response(&openai).as_deref(),
            Some("Halo juga!")
        );
        let anthropic: serde_json::Value = serde_json::from_str(
            r#"{"content":[{"type":"text","text":"Hai "},{"type":"text","text":"Astr"}]}"#,
        )
        .unwrap();
        assert_eq!(
            parse_anthropic_chat_response(&anthropic).as_deref(),
            Some("Hai Astr")
        );
        let gemini: serde_json::Value = serde_json::from_str(
            r#"{"candidates":[{"content":{"parts":[{"text":"Selamat datang"}]}}]}"#,
        )
        .unwrap();
        assert_eq!(
            parse_gemini_chat_response(&gemini).as_deref(),
            Some("Selamat datang")
        );
        // Respons kosong / malformed => None, bukan panic.
        assert!(parse_openai_chat_response(&serde_json::json!({})).is_none());
        assert!(parse_gemini_chat_response(&serde_json::json!({"candidates": []})).is_none());
    }

    #[test]
    fn test_url_building_per_provider() {
        assert_eq!(
            build_test_url("openai_compatible", Some("https://x.example/v1")).unwrap(),
            "https://x.example/v1/models"
        );
        assert!(build_test_url("openai_compatible", None).is_err());
        assert_eq!(
            build_test_url("anthropic", None).unwrap(),
            "https://api.anthropic.com/v1/models"
        );
        assert!(build_test_url("gemini", None)
            .unwrap()
            .contains("generativelanguage.googleapis.com"));
        assert!(build_test_url("unknown", None).is_err());
    }

    #[test]
    fn count_models_parses_sample_lists() {
        let openai: serde_json::Value =
            serde_json::from_str(r#"{"data":[{"id":"a"},{"id":"b"}]}"#).unwrap();
        assert_eq!(count_models("openai_compatible", &openai), Some(2));
        let gemini: serde_json::Value =
            serde_json::from_str(r#"{"models":[{"name":"m1"}]}"#).unwrap();
        assert_eq!(count_models("gemini", &gemini), Some(1));
        assert_eq!(count_models("gemini", &openai), None);
    }

    #[test]
    fn guardrail_allows_private_chat_under_limit() {
        let input = AiGuardInput {
            chat_type: "private".to_string(),
            ..Default::default()
        };
        assert!(should_ai_reply(&input).is_ok());
    }

    #[test]
    fn guardrail_rejects_group_when_not_allowed() {
        let input = AiGuardInput {
            chat_type: "group".to_string(),
            allow_groups: false,
            ..Default::default()
        };
        assert_eq!(
            should_ai_reply(&input),
            Err(AiBlockReason::GroupNotAllowed)
        );
        let mut ok = input.clone();
        ok.allow_groups = true;
        assert!(should_ai_reply(&ok).is_ok());
        // Supergroup sama aturannya dengan group.
        let mut sg = input.clone();
        sg.chat_type = "supergroup".to_string();
        sg.allow_groups = true;
        assert!(should_ai_reply(&sg).is_ok());
    }

    #[test]
    fn guardrail_rejects_bot_sender() {
        let input = AiGuardInput {
            chat_type: "private".to_string(),
            from_is_bot: true,
            ..Default::default()
        };
        assert_eq!(should_ai_reply(&input), Err(AiBlockReason::FromBot));
    }

    #[test]
    fn guardrail_rate_limit_10_per_hour() {
        let mut input = AiGuardInput {
            chat_type: "private".to_string(),
            usage_count_this_hour: 9,
            ..Default::default()
        };
        assert!(should_ai_reply(&input).is_ok());
        input.usage_count_this_hour = 10;
        assert_eq!(should_ai_reply(&input), Err(AiBlockReason::RateLimited));
        input.usage_count_this_hour = 25;
        assert_eq!(should_ai_reply(&input), Err(AiBlockReason::RateLimited));
    }

    #[test]
    fn guardrail_anti_loop_blocks_already_replied_message() {
        let input = AiGuardInput {
            chat_type: "private".to_string(),
            already_replied_to_this_message: true,
            ..Default::default()
        };
        assert_eq!(
            should_ai_reply(&input),
            Err(AiBlockReason::AlreadyReplied)
        );
    }

    #[test]
    fn hour_bucket_format() {
        let dt = chrono::DateTime::parse_from_rfc3339("2026-11-25T10:05:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(hour_bucket(dt), "2026-11-25-10");
    }
}
