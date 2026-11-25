//! Segmen CRM (v1.3): kriteria, validasi, dan pembangunan kondisi SQL.
//!
//! Bentuk criteria JSON (semua field opsional, dikombinasi AND):
//! ```json
//! {
//!   "any_tags": ["<tag-id>", "..."],
//!   "has_ordered": true,
//!   "last_active_within_days": 7,
//!   "last_active_over_days_ago": 30,
//!   "min_messages": 10
//! }
//! ```
//! Semantik:
//! - `any_tags`: user punya minimal satu dari tag tersebut.
//! - `has_ordered`: ada baris di `orders` untuk user itu. Pemetaan lewat
//!   `orders.chat_id` (TEXT) + `orders.bot_id`: hanya order ke chat privat
//!   (chat_id == telegram_user_id) yang bisa dipetakan; order grup/channel
//!   tidak dihitung — lihat catatan di migrasi v13.
//! - `last_active_within_days`: `last_seen_at` dalam X hari terakhir.
//! - `last_active_over_days_ago`: `last_seen_at` lebih dari X hari lalu.
//! - `min_messages`: `message_count >= N`.
//!
//! Evaluasi SELALU di server (SQL), bukan di frontend. `last_seen_at`
//! disimpan sebagai string RFC3339 (UTC) dengan format konsisten dari
//! `Utc::now().to_rfc3339()`, sehingga perbandingan leksikografis terhadap
//! cutoff RFC3339 valid secara kronologis.

use chrono::{DateTime, Duration, Utc};
use sqlx::{QueryBuilder, Sqlite};

use crate::error::{AppError, AppResult};

/// Kunci criteria yang dikenal — kunci lain ditolak (400) agar typo tidak
/// diam-diam diabaikan.
const KNOWN_KEYS: [&str; 5] = [
    "any_tags",
    "has_ordered",
    "last_active_within_days",
    "last_active_over_days_ago",
    "min_messages",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SegmentCriteria {
    pub any_tags: Option<Vec<String>>,
    pub has_ordered: Option<bool>,
    pub last_active_within_days: Option<i64>,
    pub last_active_over_days_ago: Option<i64>,
    pub min_messages: Option<i64>,
}

impl SegmentCriteria {
    /// Apakah ada satu pun filter aktif.
    pub fn is_empty(&self) -> bool {
        self.any_tags.is_none()
            && self.has_ordered.is_none()
            && self.last_active_within_days.is_none()
            && self.last_active_over_days_ago.is_none()
            && self.min_messages.is_none()
    }
}

/// Nilai bind untuk fragmen SQL segmen, dalam urutan kemunculan `?`.
#[derive(Debug, Clone, PartialEq)]
pub enum SegBind {
    Text(String),
    Int(i64),
}

/// Parse + validasi criteria dari JSON mentah. Kriteria tidak valid → 400
/// dengan pesan yang jelas.
pub fn parse_criteria(value: &serde_json::Value) -> AppResult<SegmentCriteria> {
    let obj = value.as_object().ok_or_else(|| {
        AppError::Validation("criteria harus berupa objek JSON".to_string())
    })?;
    for key in obj.keys() {
        if !KNOWN_KEYS.contains(&key.as_str()) {
            return Err(AppError::Validation(format!(
                "criteria tidak dikenal: '{key}' (yang didukung: any_tags, has_ordered, \
                 last_active_within_days, last_active_over_days_ago, min_messages)"
            )));
        }
    }
    let mut out = SegmentCriteria::default();
    if let Some(v) = obj.get("any_tags") {
        let arr = v.as_array().ok_or_else(|| {
            AppError::Validation("criteria.any_tags harus berupa array string tag-id".to_string())
        })?;
        if arr.is_empty() {
            return Err(AppError::Validation(
                "criteria.any_tags tidak boleh berupa array kosong".to_string(),
            ));
        }
        let mut tags = Vec::with_capacity(arr.len());
        for t in arr {
            let s = t.as_str().ok_or_else(|| {
                AppError::Validation(
                    "criteria.any_tags harus berisi string tag-id".to_string(),
                )
            })?;
            if s.trim().is_empty() {
                return Err(AppError::Validation(
                    "criteria.any_tags tidak boleh berisi tag-id kosong".to_string(),
                ));
            }
            tags.push(s.to_string());
        }
        out.any_tags = Some(tags);
    }
    if let Some(v) = obj.get("has_ordered") {
        out.has_ordered = Some(v.as_bool().ok_or_else(|| {
            AppError::Validation("criteria.has_ordered harus berupa boolean".to_string())
        })?);
    }
    if let Some(v) = obj.get("last_active_within_days") {
        let n = v.as_i64().ok_or_else(|| {
            AppError::Validation(
                "criteria.last_active_within_days harus berupa bilangan bulat (hari)".to_string(),
            )
        })?;
        if n < 0 {
            return Err(AppError::Validation(
                "criteria.last_active_within_days tidak boleh negatif".to_string(),
            ));
        }
        out.last_active_within_days = Some(n);
    }
    if let Some(v) = obj.get("last_active_over_days_ago") {
        let n = v.as_i64().ok_or_else(|| {
            AppError::Validation(
                "criteria.last_active_over_days_ago harus berupa bilangan bulat (hari)".to_string(),
            )
        })?;
        if n < 0 {
            return Err(AppError::Validation(
                "criteria.last_active_over_days_ago tidak boleh negatif".to_string(),
            ));
        }
        out.last_active_over_days_ago = Some(n);
    }
    if let Some(v) = obj.get("min_messages") {
        let n = v.as_i64().ok_or_else(|| {
            AppError::Validation(
                "criteria.min_messages harus berupa bilangan bulat".to_string(),
            )
        })?;
        if n < 0 {
            return Err(AppError::Validation(
                "criteria.min_messages tidak boleh negatif".to_string(),
            ));
        }
        out.min_messages = Some(n);
    }
    Ok(out)
}

/// Bangun fragmen kondisi SQL (` AND ...`) + nilai bind, dalam urutan `?`.
/// Semua kondisi merujuk tabel `bot_users` (query pemanggil harus memakai
/// tabel itu, tanpa alias lain).
///
/// Dipakai untuk dokumentasi/pengujian murni. Query nyata HARUS memakai
/// [`push_segment_conditions`] — `?` literal tidak boleh di-push mentah ke
/// `QueryBuilder` (placeholder hanya dibuat lewat `push_bind`).
pub fn segment_sql_fragment(
    criteria: &SegmentCriteria,
    now: DateTime<Utc>,
) -> (String, Vec<SegBind>) {
    let mut sql = String::new();
    let mut binds: Vec<SegBind> = Vec::new();
    if let Some(tags) = &criteria.any_tags {
        let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM user_tag_assignments a \
             WHERE a.user_id = bot_users.id AND a.tag_id IN ({placeholders}))"
        ));
        for t in tags {
            binds.push(SegBind::Text(t.clone()));
        }
    }
    if let Some(ordered) = criteria.has_ordered {
        // orders.chat_id adalah TEXT; untuk chat privat nilainya sama dengan
        // telegram_user_id. CAST aman: nilai non-numerik menjadi 0 dan tidak
        // pernah cocok dengan telegram_user_id (> 0).
        let exists = "EXISTS (SELECT 1 FROM orders o WHERE o.bot_id = bot_users.bot_id \
                      AND CAST(o.chat_id AS INTEGER) = bot_users.telegram_user_id)";
        if ordered {
            sql.push_str(&format!(" AND {exists}"));
        } else {
            sql.push_str(&format!(" AND NOT {exists}"));
        }
    }
    if let Some(days) = criteria.last_active_within_days {
        let cutoff = (now - Duration::days(days)).to_rfc3339();
        sql.push_str(" AND bot_users.last_seen_at IS NOT NULL AND bot_users.last_seen_at >= ?");
        binds.push(SegBind::Text(cutoff));
    }
    if let Some(days) = criteria.last_active_over_days_ago {
        let cutoff = (now - Duration::days(days)).to_rfc3339();
        sql.push_str(" AND bot_users.last_seen_at IS NOT NULL AND bot_users.last_seen_at < ?");
        binds.push(SegBind::Text(cutoff));
    }
    if let Some(min) = criteria.min_messages {
        sql.push_str(" AND bot_users.message_count >= ?");
        binds.push(SegBind::Int(min));
    }
    (sql, binds)
}

/// Terapkan kondisi segmen ke QueryBuilder yang sudah dimulai dengan
/// `... FROM bot_users WHERE 1=1` (atau kondisi lain yang sudah ada).
/// Placeholder dibuat lewat `push_bind` (bukan `?` literal).
pub fn push_segment_conditions<'a>(
    qb: &mut QueryBuilder<'a, Sqlite>,
    criteria: &'a SegmentCriteria,
    now: DateTime<Utc>,
) {
    if let Some(tags) = &criteria.any_tags {
        qb.push(
            " AND EXISTS (SELECT 1 FROM user_tag_assignments a \
             WHERE a.user_id = bot_users.id AND a.tag_id IN (",
        );
        {
            let mut separated = qb.separated(", ");
            for t in tags {
                separated.push_bind(t);
            }
            separated.push_unseparated("))");
        }
    }
    if let Some(ordered) = criteria.has_ordered {
        let exists = "EXISTS (SELECT 1 FROM orders o WHERE o.bot_id = bot_users.bot_id \
                      AND CAST(o.chat_id AS INTEGER) = bot_users.telegram_user_id)";
        if ordered {
            qb.push(format!(" AND {exists}"));
        } else {
            qb.push(format!(" AND NOT {exists}"));
        }
    }
    if let Some(days) = criteria.last_active_within_days {
        let cutoff = (now - Duration::days(days)).to_rfc3339();
        qb.push(" AND bot_users.last_seen_at IS NOT NULL AND bot_users.last_seen_at >= ")
            .push_bind(cutoff);
    }
    if let Some(days) = criteria.last_active_over_days_ago {
        let cutoff = (now - Duration::days(days)).to_rfc3339();
        qb.push(" AND bot_users.last_seen_at IS NOT NULL AND bot_users.last_seen_at < ")
            .push_bind(cutoff);
    }
    if let Some(min) = criteria.min_messages {
        qb.push(" AND bot_users.message_count >= ").push_bind(min);
    }
}

/// Validasi warna hex `#RRGGBB` (dipakai tag).
pub fn is_valid_hex_color(color: &str) -> bool {
    let b = color.as_bytes();
    b.len() == 7
        && b[0] == b'#'
        && b[1..].iter().all(|c| c.is_ascii_hexdigit())
}

/// Validasi `violation_action` group_settings.
pub fn is_valid_violation_action(action: &str) -> bool {
    matches!(action, "delete" | "warn" | "mute")
}

// ---------------------------------------------------------------------------
// Query anggota segmen (dipakai handler segmen + filter GET /api/users)
// ---------------------------------------------------------------------------

/// Hitung anggota segmen (opsional dibatasi satu bot).
pub async fn segment_member_count(
    pool: &sqlx::SqlitePool,
    criteria: &SegmentCriteria,
    bot_id: Option<&str>,
) -> AppResult<i64> {
    let mut qb = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM bot_users WHERE 1=1");
    if let Some(b) = bot_id {
        qb.push(" AND bot_id = ").push_bind(b);
    }
    push_segment_conditions(&mut qb, criteria, Utc::now());
    Ok(qb.build_query_scalar::<i64>().fetch_one(pool).await?)
}

/// Ambil anggota segmen (opsional dibatasi satu bot), terbaru dulu.
pub async fn segment_members(
    pool: &sqlx::SqlitePool,
    criteria: &SegmentCriteria,
    bot_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<crate::models::BotUser>> {
    let mut qb =
        QueryBuilder::<Sqlite>::new("SELECT * FROM bot_users WHERE 1=1");
    if let Some(b) = bot_id {
        qb.push(" AND bot_id = ").push_bind(b);
    }
    push_segment_conditions(&mut qb, criteria, Utc::now());
    qb.push(" ORDER BY last_seen_at DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    Ok(qb
        .build_query_as::<crate::models::BotUser>()
        .fetch_all(pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-25T12:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn parses_all_five_criteria_kinds() {
        let c = parse_criteria(&json!({
            "any_tags": ["t1", "t2"],
            "has_ordered": true,
            "last_active_within_days": 7,
            "last_active_over_days_ago": 30,
            "min_messages": 10,
        }))
        .unwrap();
        assert_eq!(c.any_tags, Some(vec!["t1".to_string(), "t2".to_string()]));
        assert_eq!(c.has_ordered, Some(true));
        assert_eq!(c.last_active_within_days, Some(7));
        assert_eq!(c.last_active_over_days_ago, Some(30));
        assert_eq!(c.min_messages, Some(10));
        assert!(!c.is_empty());
    }

    #[test]
    fn empty_object_is_valid_and_empty() {
        let c = parse_criteria(&json!({})).unwrap();
        assert!(c.is_empty());
        let (sql, binds) = segment_sql_fragment(&c, now());
        assert!(sql.is_empty());
        assert!(binds.is_empty());
    }

    #[test]
    fn rejects_unknown_key() {
        let err = parse_criteria(&json!({"never_active": true})).unwrap_err();
        assert!(err.to_string().contains("tidak dikenal"));
    }

    #[test]
    fn rejects_non_object_criteria() {
        assert!(parse_criteria(&json!("tag")).is_err());
        assert!(parse_criteria(&json!([1, 2])).is_err());
    }

    #[test]
    fn rejects_empty_any_tags_and_bad_types() {
        assert!(parse_criteria(&json!({"any_tags": []})).is_err());
        assert!(parse_criteria(&json!({"any_tags": "t1"})).is_err());
        assert!(parse_criteria(&json!({"any_tags": [""]})).is_err());
        assert!(parse_criteria(&json!({"has_ordered": "yes"})).is_err());
        assert!(parse_criteria(&json!({"min_messages": 1.5})).is_err());
    }

    #[test]
    fn rejects_negative_numbers() {
        assert!(parse_criteria(&json!({"last_active_within_days": -1})).is_err());
        assert!(parse_criteria(&json!({"last_active_over_days_ago": -5})).is_err());
        assert!(parse_criteria(&json!({"min_messages": -1})).is_err());
    }

    #[test]
    fn sql_fragment_combines_all_kinds_with_and() {
        let c = parse_criteria(&json!({
            "any_tags": ["t1", "t2"],
            "has_ordered": true,
            "last_active_within_days": 7,
            "last_active_over_days_ago": 30,
            "min_messages": 10,
        }))
        .unwrap();
        let (sql, binds) = segment_sql_fragment(&c, now());
        assert!(sql.contains("user_tag_assignments"));
        assert!(sql.contains("IN (?,?)"));
        assert!(sql.contains("EXISTS (SELECT 1 FROM orders"));
        assert!(sql.contains("last_seen_at >= ?"));
        assert!(sql.contains("last_seen_at < ?"));
        assert!(sql.contains("message_count >= ?"));
        assert_eq!(binds.len(), 5);
        assert_eq!(binds[0], SegBind::Text("t1".to_string()));
        assert_eq!(binds[1], SegBind::Text("t2".to_string()));
        // Cutoff 7 hari sebelum 2026-11-25T12:00:00Z.
        assert_eq!(binds[2], SegBind::Text("2026-11-18T12:00:00+00:00".to_string()));
        assert_eq!(binds[3], SegBind::Text("2026-10-26T12:00:00+00:00".to_string()));
        assert_eq!(binds[4], SegBind::Int(10));
    }

    #[test]
    fn sql_fragment_has_ordered_false_uses_not_exists() {
        let c = parse_criteria(&json!({"has_ordered": false})).unwrap();
        let (sql, _) = segment_sql_fragment(&c, now());
        assert!(sql.contains("NOT EXISTS (SELECT 1 FROM orders"));
    }

    #[test]
    fn hex_color_validation() {
        assert!(is_valid_hex_color("#526FE5"));
        assert!(is_valid_hex_color("#ffffff"));
        assert!(is_valid_hex_color("#000000"));
        assert!(!is_valid_hex_color("526FE5"));
        assert!(!is_valid_hex_color("#526FE"));
        assert!(!is_valid_hex_color("#526FE55"));
        assert!(!is_valid_hex_color("#ZZZZZZ"));
        assert!(!is_valid_hex_color(""));
    }

    #[test]
    fn violation_action_validation() {
        assert!(is_valid_violation_action("delete"));
        assert!(is_valid_violation_action("warn"));
        assert!(is_valid_violation_action("mute"));
        assert!(!is_valid_violation_action("ban"));
        assert!(!is_valid_violation_action(""));
    }

    // --- Test integrasi DB (in-memory + migrasi): evaluasi segmen ---

    async fn test_pool() -> sqlx::SqlitePool {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
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
        pool
    }

    fn days_ago(n: i64) -> String {
        (Utc::now() - Duration::days(n)).to_rfc3339()
    }

    /// Fixture: bot-1 + 4 user.
    /// u1: recent, 12 pesan, tag t1. u2: 40 hari lalu, 3 pesan, tanpa tag.
    /// u3: 2 hari lalu, 20 pesan, tag t2, punya order (chat privat).
    /// u4: last_seen NULL, 0 pesan, tanpa tag.
    async fn seed_segment_fixture(pool: &sqlx::SqlitePool) {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO bots (id, name, token_encrypted, token_masked, status, created_at, updated_at) \
             VALUES ('bot-1', 'Bot Test', 'enc', '****', 'disconnected', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
        for (id, tg, count, last_seen) in [
            ("u1", 111i64, 12i64, Some(days_ago(0))),
            ("u2", 222i64, 3i64, Some(days_ago(40))),
            ("u3", 333i64, 20i64, Some(days_ago(2))),
            ("u4", 444i64, 0i64, None),
        ] {
            sqlx::query(
                "INSERT INTO bot_users (id, bot_id, telegram_user_id, first_name, \
                 message_count, last_seen_at, created_at, updated_at) \
                 VALUES (?, 'bot-1', ?, 'User', ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(tg)
            .bind(count)
            .bind(last_seen)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO user_tags (id, name, color, created_at) VALUES \
             ('t1', 'VIP', '#526FE5', ?), ('t2', 'Baru', '#00AA00', ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
        for (user_id, tag_id) in [("u1", "t1"), ("u3", "t2")] {
            sqlx::query(
                "INSERT INTO user_tag_assignments (user_id, tag_id, assigned_at) VALUES (?, ?, ?)",
            )
            .bind(user_id)
            .bind(tag_id)
            .bind(&now)
            .execute(pool)
            .await
            .unwrap();
        }
        // Order untuk u3: invoice dikirim ke chat privat 333.
        sqlx::query(
            "INSERT INTO orders (id, bot_id, chat_id, invoice_payload, amount, currency, status, created_at) \
             VALUES ('o1', 'bot-1', '333', 'payload#o1', 5, 'XTR', 'paid', ?)",
        )
        .bind(&now)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn count(pool: &sqlx::SqlitePool, criteria: serde_json::Value) -> i64 {
        let c = parse_criteria(&criteria).unwrap();
        segment_member_count(pool, &c, None).await.unwrap()
    }

    #[tokio::test]
    async fn segment_any_tags_counts_users_with_at_least_one() {
        let pool = test_pool().await;
        seed_segment_fixture(&pool).await;
        assert_eq!(count(&pool, json!({"any_tags": ["t1"]})).await, 1);
        assert_eq!(count(&pool, json!({"any_tags": ["t1", "t2"]})).await, 2);
        assert_eq!(count(&pool, json!({"any_tags": ["tidak-ada"]})).await, 0);
    }

    #[tokio::test]
    async fn segment_has_ordered_matches_private_chat_orders_only() {
        let pool = test_pool().await;
        seed_segment_fixture(&pool).await;
        assert_eq!(count(&pool, json!({"has_ordered": true})).await, 1);
        assert_eq!(count(&pool, json!({"has_ordered": false})).await, 3);
    }

    #[tokio::test]
    async fn segment_last_active_windows() {
        let pool = test_pool().await;
        seed_segment_fixture(&pool).await;
        assert_eq!(count(&pool, json!({"last_active_within_days": 7})).await, 2);
        assert_eq!(
            count(&pool, json!({"last_active_over_days_ago": 30})).await,
            1
        );
        // u4 (last_seen NULL) tidak masuk ke kedua jendela.
        assert_eq!(count(&pool, json!({"last_active_within_days": 365})).await, 3);
    }

    #[tokio::test]
    async fn segment_min_messages() {
        let pool = test_pool().await;
        seed_segment_fixture(&pool).await;
        assert_eq!(count(&pool, json!({"min_messages": 10})).await, 2);
        assert_eq!(count(&pool, json!({"min_messages": 0})).await, 4);
    }

    #[tokio::test]
    async fn segment_and_combination() {
        let pool = test_pool().await;
        seed_segment_fixture(&pool).await;
        assert_eq!(
            count(
                &pool,
                json!({"any_tags": ["t2"], "has_ordered": true, "min_messages": 10})
            )
            .await,
            1
        );
        assert_eq!(
            count(&pool, json!({"any_tags": ["t1"], "has_ordered": true})).await,
            0
        );
        assert_eq!(
            count(
                &pool,
                json!({"last_active_within_days": 7, "min_messages": 15})
            )
            .await,
            1
        );
    }

    #[tokio::test]
    async fn segment_members_returns_matching_users() {
        let pool = test_pool().await;
        seed_segment_fixture(&pool).await;
        let c = parse_criteria(&json!({"min_messages": 10})).unwrap();
        let members = segment_members(&pool, &c, None, 20, 0).await.unwrap();
        assert_eq!(members.len(), 2);
        let ids: Vec<&str> = members.iter().map(|u| u.id.as_str()).collect();
        assert!(ids.contains(&"u1") && ids.contains(&"u3"));
        // Filter bot yang tidak ada -> kosong.
        let none = segment_members(&pool, &c, Some("bot-lain"), 20, 0)
            .await
            .unwrap();
        assert!(none.is_empty());
    }
}
