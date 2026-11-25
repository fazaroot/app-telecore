//! Penegakan moderasi grup v1.3 (dipakai `process_update`).
//!
//! Aturan hanya berlaku untuk chat grup/supergrup dan hanya bila pengaturan
//! di `group_settings` aktif. Pesan privat tidak pernah disentuh.
//!
//! Keputusan pelanggaran dipisah menjadi fungsi murni (`detect_violations`)
//! agar bisa diuji unit tanpa Telegram maupun database; eksekusi (hapus /
//! mute / peringatan) dan pencatatan log ada di `update_processor`.

use chrono::{Duration, Utc};

use crate::error::AppResult;
use crate::models::{new_id, now_rfc3339, GroupSettings};
use crate::services::log_activity;
use crate::state::AppState;

/// Jenis pelanggaran yang terdeteksi pada satu pesan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Violation {
    /// Teks mengandung salah satu kata terlarang.
    BannedWord,
    /// Teks mengandung link (http://, https://, atau t.me/).
    Link,
    /// Pengirim melewati batas flood dalam jendela waktu.
    Flood,
}

/// Apakah teks mengandung link. Case-insensitive (skema URL huruf kecil
/// setelah `to_lowercase`; `t.me/` juga dicek dalam bentuk lowercase).
pub fn contains_link(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("http://") || lower.contains("https://") || lower.contains("t.me/")
}

/// Kata terlarang pertama yang cocok (case-insensitive, substring).
/// Mengembalikan kata dari daftar (bentuk aslinya) bila ada yang cocok.
pub fn matching_banned_word<'a>(text: &str, banned_words: &'a [String]) -> Option<&'a str> {
    let lower = text.to_lowercase();
    banned_words
        .iter()
        .find(|w| {
            let w = w.trim().to_lowercase();
            !w.is_empty() && lower.contains(&w)
        })
        .map(|w| w.as_str())
}

/// Parse kolom `banned_words` (JSON array string) menjadi daftar kata.
pub fn parse_banned_words(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

/// Deteksi pelanggaran berbasis teks saja (kata terlarang + link).
/// Flood dicek terpisah karena butuh hitungan dari tabel `messages`.
pub fn detect_text_violations(text: &str, settings: &GroupSettings) -> Vec<Violation> {
    let mut out = Vec::new();
    let banned = parse_banned_words(&settings.banned_words);
    if !banned.is_empty() && matching_banned_word(text, &banned).is_some() {
        out.push(Violation::BannedWord);
    }
    if settings.block_links != 0 && contains_link(text) {
        out.push(Violation::Link);
    }
    out
}

/// Apakah ada satu pun penegakan v1.3 yang aktif di pengaturan ini.
pub fn enforcement_active(settings: &GroupSettings) -> bool {
    settings.anti_spam_enabled != 0
        || settings.block_links != 0
        || !parse_banned_words(&settings.banned_words).is_empty()
}

/// Hitung pesan inbound dari user di chat ini dalam `window_seconds`
/// terakhir, dari tabel `messages` (pesan saat ini sudah tersimpan, jadi
/// ikut terhitung). `created_at` RFC3339 dibandingkan leksikografis
/// terhadap cutoff RFC3339 — valid karena formatnya konsisten (UTC).
pub async fn flood_count(
    state: &AppState,
    bot_id: &str,
    chat_id: i64,
    user_id: i64,
    window_seconds: i64,
) -> AppResult<i64> {
    let cutoff = (Utc::now() - Duration::seconds(window_seconds.max(1))).to_rfc3339();
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM messages WHERE bot_id = ? AND chat_id = ? \
         AND from_user_id = ? AND direction = 'inbound' AND created_at >= ?",
    )
    .bind(bot_id)
    .bind(chat_id)
    .bind(user_id)
    .bind(cutoff)
    .fetch_one(&state.pool)
    .await?;
    Ok(count)
}

/// Catat satu aksi moderasi ke `group_moderation_log` DAN `activity_logs`
/// (action 'moderation'). Dipakai aksi API maupun otomatis. Tidak pernah
/// gagalkan pemanggil: error DB diabaikan (sudah di-log via tracing).
pub async fn record_moderation(
    state: &AppState,
    bot_id: &str,
    chat_id: i64,
    user_id: Option<i64>,
    action: &str,
    reason: Option<&str>,
    details: serde_json::Value,
) {
    let now = now_rfc3339();
    let insert = sqlx::query(
        "INSERT INTO group_moderation_log (id, chat_id, user_id, action, reason, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(chat_id)
    .bind(user_id)
    .bind(action)
    .bind(reason)
    .bind(&now)
    .execute(&state.pool)
    .await;
    if let Err(e) = insert {
        tracing::warn!("Gagal mencatat group_moderation_log: {e}");
    }
    log_activity(
        state,
        Some(bot_id),
        "moderation",
        Some("moderation"),
        Some(&chat_id.to_string()),
        details,
        "warning",
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GroupSettings;

    fn settings() -> GroupSettings {
        GroupSettings {
            chat_id: -100123,
            welcome_enabled: 0,
            welcome_message: None,
            anti_spam_enabled: 1,
            flood_limit: 5,
            flood_window_seconds: 10,
            block_links: 1,
            banned_words: serde_json::to_string(&["spam", "JUDI"]).unwrap(),
            violation_action: "delete".to_string(),
            mute_duration_minutes: 60,
        }
    }

    #[test]
    fn banned_word_matches_case_insensitive_substring() {
        assert_eq!(
            matching_banned_word("ini JUDI online", &["judi".to_string()]),
            Some("judi")
        );
        assert_eq!(
            matching_banned_word("spammer datang", &["spam".to_string()]),
            Some("spam")
        );
        assert_eq!(matching_banned_word("halo semua", &["spam".to_string()]), None);
        assert_eq!(matching_banned_word("halo", &["".to_string(), "  ".to_string()]), None);
    }

    #[test]
    fn link_detection_covers_schemes_and_tme() {
        assert!(contains_link("cek https://example.com"));
        assert!(contains_link("cek HTTP://EXAMPLE.COM"));
        assert!(contains_link("join t.me/grupku"));
        assert!(contains_link("join T.ME/GrupKu"));
        assert!(!contains_link("tidak ada link di sini"));
        assert!(!contains_link("www.example.com tanpa skema"));
    }

    #[test]
    fn text_violations_detect_both_kinds() {
        let s = settings();
        assert_eq!(
            detect_text_violations("main JUDI di https://x.com", &s),
            vec![Violation::BannedWord, Violation::Link]
        );
        assert_eq!(detect_text_violations("halo semua", &s), Vec::<Violation>::new());
        let mut off = settings();
        off.block_links = 0;
        off.banned_words = "[]".to_string();
        assert_eq!(detect_text_violations("judi https://x.com", &off), Vec::<Violation>::new());
    }

    #[test]
    fn enforcement_active_requires_a_rule() {
        let mut s = settings();
        assert!(enforcement_active(&s));
        s.anti_spam_enabled = 0;
        s.block_links = 0;
        s.banned_words = "[]".to_string();
        assert!(!enforcement_active(&s));
    }

    #[test]
    fn parse_banned_words_tolerates_bad_json() {
        assert_eq!(parse_banned_words("[]"), Vec::<String>::new());
        assert_eq!(parse_banned_words("bukan json"), Vec::<String>::new());
        assert_eq!(
            parse_banned_words(r#"["a","b"]"#),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
