//! Model database (SQLx `FromRow`) + helper umum.
//!
//! Catatan serialisasi API (kontrak dengan frontend):
//! - Kolom JSON yang disimpan sebagai TEXT di SQLite diserialisasi sebagai
//!   nilai JSON asli (objek/array), bukan string — via `ser_json_str`.
//! - Flag 0/1 (INTEGER di DB) diserialisasi sebagai boolean — via `ser_i64_bool`.
//! - `Bot.status` diserialisasi sebagai `connection_status` (nama yang dipakai UI).

use chrono::Utc;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Timestamp sekarang sebagai string RFC3339 (UTC).
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

/// ID baru (UUID v4).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Serialisasi kolom JSON-TEXT sebagai nilai JSON asli.
/// Bila string bukan JSON valid, fallback ke string apa adanya.
pub fn ser_json_str<S>(value: &String, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let parsed: serde_json::Value =
        serde_json::from_str(value).unwrap_or(serde_json::Value::String(value.clone()));
    parsed.serialize(serializer)
}

/// Serialisasi flag INTEGER 0/1 sebagai boolean.
pub fn ser_i64_bool<S>(value: &i64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_bool(*value != 0)
}

/// Deserializer `chat_id` yang toleran: terima angka JSON (`123`) maupun
/// string angka (`"123"`, `"-100123"` — ID grup negatif). Username
/// (`"@nama"`) ditolak: v1 hanya mendukung ID numerik.
pub fn de_chat_id<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    struct ChatIdVisitor;
    impl serde::de::Visitor<'_> for ChatIdVisitor {
        type Value = i64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("chat_id berupa angka atau string angka")
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<i64, E> {
            i64::try_from(v).map_err(|_| E::custom("chat_id terlalu besar"))
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<i64, E> {
            v.trim().parse::<i64>().map_err(|_| {
                E::custom("chat_id harus berupa angka (ID numerik Telegram, bukan @username)")
            })
        }
    }
    deserializer.deserialize_any(ChatIdVisitor)
}

/// Versi opsional dari [`de_chat_id`] (untuk field `Option<i64>`).
pub fn de_opt_chat_id<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(serde_json::Value::Number(n)) => n
            .as_i64()
            .ok_or_else(|| serde::de::Error::custom("chat_id terlalu besar"))
            .map(Some),
        Some(serde_json::Value::String(s)) => s
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(|_| serde::de::Error::custom("chat_id harus berupa angka")),
        Some(_) => Err(serde::de::Error::custom("chat_id harus berupa angka")),
    }
}

/// Deserializer `media_type`: `"text"` / `"none"` / string kosong dianggap
/// tidak ada (pesan teks biasa).
pub fn de_media_type<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.filter(|s| {
        let t = s.trim();
        !t.is_empty() && t != "text" && t != "none"
    }))
}

/// Deserializer `parse_mode`: `"none"` / string kosong dianggap tidak ada.
pub fn de_parse_mode<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.filter(|s| {
        let t = s.trim();
        !t.is_empty() && t != "none"
    }))
}

// ---------------------------------------------------------------------------
// Bots
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Bot {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub telegram_id: Option<i64>,
    /// TIDAK PERNAH diserialisasi ke API — hanya dipakai internal untuk dekripsi.
    #[serde(skip_serializing)]
    pub token_encrypted: String,
    pub token_masked: String,
    #[serde(rename = "connection_status")]
    pub status: String,
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_active: i64,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// v1.2 (watchdog): terakhir kali getMe dicek.
    pub last_check_at: Option<String>,
    /// v1.2 (watchdog): jumlah kegagalan getMe berturut-turut.
    pub consecutive_failures: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BotConfiguration {
    pub id: String,
    pub bot_id: String,
    pub webhook_url: Option<String>,
    #[serde(serialize_with = "ser_i64_bool")]
    pub polling_enabled: i64,
    #[serde(serialize_with = "ser_i64_bool")]
    pub webhook_enabled: i64,
    #[serde(skip_serializing)]
    pub webhook_secret: Option<String>,
    pub last_update_offset: i64,
    #[serde(serialize_with = "ser_i64_bool")]
    pub auto_reply_enabled: i64,
    pub welcome_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// v1.2 (AI Smart Reply).
    pub ai_provider_id: Option<String>,
    #[serde(serialize_with = "ser_i64_bool")]
    pub ai_enabled: i64,
    #[serde(serialize_with = "ser_i64_bool")]
    pub ai_allow_groups: i64,
    pub ai_system_prompt: Option<String>,
    pub ai_max_tokens: i64,
    pub ai_temperature: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BotCommand {
    pub id: String,
    pub bot_id: String,
    pub name: String,
    pub description: String,
    pub response_template: String,
    #[serde(serialize_with = "ser_json_str")]
    pub aliases: String, // JSON array string
    #[serde(serialize_with = "ser_json_str")]
    pub permissions: String, // JSON array string
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_enabled: i64,
    pub usage_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BotUser {
    pub id: String,
    pub bot_id: String,
    pub telegram_user_id: i64,
    pub username: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub language_code: Option<String>,
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_blocked: i64,
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_bot: i64,
    pub notes: Option<String>,
    #[serde(serialize_with = "ser_json_str")]
    pub labels: String, // JSON array string
    pub last_seen_at: Option<String>,
    pub message_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BotChat {
    pub id: String,
    pub bot_id: String,
    pub chat_id: i64,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub chat_type: String,
    pub title: Option<String>,
    pub username: Option<String>,
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_bot_admin: i64,
    #[serde(serialize_with = "ser_json_str")]
    pub settings: String, // JSON object string
    pub member_count: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub id: String,
    pub bot_id: String,
    pub chat_id: i64,
    pub telegram_message_id: Option<i64>,
    pub direction: String,
    pub message_type: String,
    pub content: Option<String>,
    pub media_file_id: Option<String>,
    pub status: String,
    pub from_user_id: Option<i64>,
    pub reply_to_message_id: Option<i64>,
    pub parse_mode: Option<String>,
    pub error_message: Option<String>,
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_read: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MessageTemplate {
    pub id: String,
    pub bot_id: String,
    pub name: String,
    pub content: String,
    pub category: Option<String>,
    #[serde(serialize_with = "ser_json_str")]
    pub variables: String, // JSON array string
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AutomationWorkflow {
    pub id: String,
    pub bot_id: String,
    pub name: String,
    pub description: Option<String>,
    pub trigger_type: String,
    #[serde(serialize_with = "ser_json_str")]
    pub trigger_config: String, // JSON object string
    #[serde(serialize_with = "ser_json_str")]
    pub conditions: String, // JSON (array rules atau {logic, rules})
    #[serde(serialize_with = "ser_json_str")]
    pub actions: String, // JSON array
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_enabled: i64,
    pub execution_count: i64,
    pub last_executed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AutomationExecution {
    pub id: String,
    pub workflow_id: String,
    pub bot_id: String,
    pub trigger_event: Option<String>,
    pub status: String,
    pub steps_executed: i64,
    pub error_message: Option<String>,
    pub duration_ms: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ScheduledTask {
    pub id: String,
    pub bot_id: String,
    pub name: String,
    pub description: Option<String>,
    pub task_type: String, // one-time | daily | weekly | interval | cron
    pub chat_id: i64,
    pub message_content: String,
    pub media_url: Option<String>,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub timezone: String,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub run_count: i64,
    pub failure_count: i64,
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_active: i64,
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_paused: i64,
    /// v1.2: target pengiriman khusus, mis. "channel" + target_id = chat_id
    /// channel (untuk posting channel terjadwal). NULL = kirim biasa ke chat_id.
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notification {
    pub id: String,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub notification_type: String,
    pub title: String,
    pub message: String,
    #[serde(serialize_with = "ser_json_str")]
    pub data: String, // JSON object string
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_read: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ActivityLog {
    pub id: String,
    pub bot_id: Option<String>,
    pub action: String,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    #[serde(serialize_with = "ser_json_str")]
    pub details: String, // JSON object string
    pub level: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AppSetting {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Backup {
    pub id: String,
    pub filename: String,
    pub size_bytes: i64,
    pub description: Option<String>,
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_automatic: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdminSession {
    pub id: String,
    #[serde(skip_serializing)]
    pub pin_hash: Option<String>,
    #[serde(skip_serializing)]
    pub salt: Option<String>,
    #[serde(serialize_with = "ser_i64_bool")]
    pub is_locked: i64,
    pub created_at: String,
    pub updated_at: String,
    pub last_unlocked_at: Option<String>,
}

// ---------------------------------------------------------------------------
// v1.1: Outgoing webhooks + personal API tokens
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OutgoingWebhook {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(serialize_with = "ser_json_str")]
    pub events: String, // JSON array string
    /// TIDAK PERNAH diserialisasi — secret terenkripsi AES-256-GCM.
    #[serde(skip_serializing)]
    pub secret_encrypted: Option<String>,
    #[serde(serialize_with = "ser_i64_bool")]
    pub enabled: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WebhookDelivery {
    pub id: String,
    pub webhook_id: String,
    pub event: String,
    #[serde(serialize_with = "ser_json_str")]
    pub payload: String, // JSON object string
    pub status: String,  // pending | delivered | failed
    pub http_status: Option<i64>,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub prefix: String,
    /// TIDAK PERNAH diserialisasi — SHA-256 hex dari token penuh.
    #[serde(skip_serializing)]
    pub token_hash: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

// ---------------------------------------------------------------------------
// v1.2: AI providers, produk & order Stars, AI usage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    pub provider_type: String, // openai_compatible | anthropic | gemini
    pub base_url: Option<String>,
    pub model: String,
    /// TIDAK PERNAH diserialisasi — API key terenkripsi AES-256-GCM.
    #[serde(skip_serializing)]
    pub api_key_encrypted: Option<String>,
    pub api_key_last4: Option<String>,
    #[serde(serialize_with = "ser_i64_bool")]
    pub enabled: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Product {
    pub id: String,
    pub bot_id: String,
    pub title: String,
    pub description: Option<String>,
    pub payload: String,
    /// Satuan terkecil: Stars = jumlah Stars; fiat = cents.
    pub price: i64,
    pub currency: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Order {
    pub id: String,
    pub bot_id: String,
    pub product_id: Option<String>,
    pub chat_id: String,
    pub invoice_payload: String,
    pub telegram_payment_charge_id: Option<String>,
    pub provider_payment_charge_id: Option<String>,
    pub amount: i64,
    pub currency: String,
    pub status: String, // pending | paid | failed | cancelled
    pub created_at: String,
    pub paid_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AiUsage {
    pub bot_id: String,
    pub user_id: String,
    pub hour_bucket: String,
    pub count: i64,
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
pub struct Pagination {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

impl Pagination {
    pub fn normalized(&self) -> (i64, i64) {
        let page = self.page.unwrap_or(1).max(1);
        let per_page = self.per_page.unwrap_or(20).clamp(1, 100);
        (page, per_page)
    }

    pub fn offset(&self) -> i64 {
        let (page, per_page) = self.normalized();
        (page - 1) * per_page
    }
}

#[derive(Debug, Serialize)]
pub struct Paginated<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_serializes_connection_status_and_bool_flags() {
        let bot = Bot {
            id: "b1".to_string(),
            name: "Test".to_string(),
            username: Some("testbot".to_string()),
            telegram_id: Some(123),
            token_encrypted: "SECRET".to_string(),
            token_masked: "****0123".to_string(),
            status: "connected".to_string(),
            is_active: 1,
            description: None,
            created_at: "2026-11-25T00:00:00Z".to_string(),
            updated_at: "2026-11-25T00:00:00Z".to_string(),
            last_check_at: None,
            consecutive_failures: 0,
        };
        let v: serde_json::Value = serde_json::to_value(&bot).unwrap();
        assert_eq!(v["connection_status"], "connected");
        assert!(v.get("status").is_none());
        assert_eq!(v["is_active"], true);
        // Token terenkripsi tidak pernah keluar via API.
        assert!(v.get("token_encrypted").is_none());
        assert_eq!(v["token_masked"], "****0123");
    }

    #[test]
    fn workflow_json_string_fields_serialize_as_values() {
        let wf = AutomationWorkflow {
            id: "w1".to_string(),
            bot_id: "b1".to_string(),
            name: "Welcome".to_string(),
            description: None,
            trigger_type: "keyword".to_string(),
            trigger_config: r#"{"keywords":["halo"]}"#.to_string(),
            conditions: r#"[{"field":"text","operator":"contains","value":"halo"}]"#.to_string(),
            actions: r#"[{"type":"send_message","text":"Hai!"}]"#.to_string(),
            is_enabled: 0,
            execution_count: 0,
            last_executed_at: None,
            created_at: "2026-11-25T00:00:00Z".to_string(),
            updated_at: "2026-11-25T00:00:00Z".to_string(),
        };
        let v: serde_json::Value = serde_json::to_value(&wf).unwrap();
        assert_eq!(v["trigger_config"]["keywords"][0], "halo");
        assert!(v["conditions"].is_array());
        assert_eq!(v["actions"][0]["type"], "send_message");
        assert_eq!(v["is_enabled"], false);
    }

    #[test]
    fn chat_id_deserializer_accepts_number_and_numeric_string() {
        #[derive(Debug, Deserialize)]
        struct Probe {
            #[serde(deserialize_with = "de_chat_id")]
            chat_id: i64,
        }
        let p: Probe = serde_json::from_str(r#"{"chat_id": 123}"#).unwrap();
        assert_eq!(p.chat_id, 123);
        let p: Probe = serde_json::from_str(r#"{"chat_id": "-100456"}"#).unwrap();
        assert_eq!(p.chat_id, -100456);
        assert!(serde_json::from_str::<Probe>(r#"{"chat_id": "@user"}"#).is_err());
    }
}
