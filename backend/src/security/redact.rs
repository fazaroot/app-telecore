//! Redaksi secret untuk teks bebas, JSON, dan pesan error.
//!
//! Modul ini adalah satu-satunya tempat pola redaksi didefinisikan.
//! Pemanggil (klien Telegram, notifikasi, activity log, serialisasi API,
//! pembersihan DB lama) memakai fungsi di sini agar token bot tidak bocor
//! lewat error reqwest (URL request mengandung token), metadata JSON,
//! maupun baris lama yang sudah tersimpan sebelum perbaikan ini.

/// Placeholder pengganti secret yang teredaksi.
pub const REDACTED: &str = "****";

/// Redaksi secret dalam teks bebas:
/// - pola token bot Telegram (`<digit>:<secret>`),
/// - nilai setelah key sensitif (`token=...`, `api_key: ...`,
///   `"token": "..."`, `X-API-Key: ...`, dll).
pub fn redact_secrets(input: &str) -> String {
    let mut out = redact_bot_token_pattern(input);
    for key in [
        "token",
        "api_key",
        "api-key",
        "x-api-key",
        "apikey",
        "secret",
        "password",
        "passphrase",
        "authorization",
        "pin",
    ] {
        out = mask_after_key(&out, key);
    }
    out
}

/// Redaksi teks yang mungkin mengandung secret tertentu yang diketahui
/// pemanggil (mis. token bot yang sedang dipakai klien Telegram).
/// Penggantian persis dilakukan dulu agar token pendek sekalipun tidak
/// lolos, lalu redaksi generik diterapkan.
pub fn redact_known_secret(input: &str, secret: &str) -> String {
    if secret.is_empty() {
        return redact_secrets(input);
    }
    redact_secrets(&input.replace(secret, REDACTED))
}

/// Redaksi nilai JSON secara rekursif. String apa pun diredaksi generik;
/// nilai di bawah key sensitif (token/api_key/secret/...) diganti penuh
/// dengan placeholder, kecuali key yang memang sudah bentuk masking
/// (mis. `token_masked`).
pub fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            *s = redact_secrets(s);
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        serde_json::Value::Object(map) => {
            let old = std::mem::take(map);
            for (key, mut item) in old {
                let redacted_key = redact_secrets(&key);
                if is_sensitive_key(&key) {
                    item = serde_json::Value::String(REDACTED.to_string());
                } else {
                    redact_json_value(&mut item);
                }
                map.insert(redacted_key, item);
            }
        }
        _ => {}
    }
}

/// Redaksi string yang seharusnya berisi JSON (kolom TEXT di SQLite).
/// Bila bukan JSON valid, teks diredaksi sebagai teks bebas.
pub fn redact_json_text(input: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(input) {
        Ok(mut value) => {
            redact_json_value(&mut value);
            value.to_string()
        }
        Err(_) => redact_secrets(input),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    // Bentuk masking yang memang aman ditampilkan.
    if normalized.ends_with("masked") {
        return false;
    }
    matches!(
        normalized.as_str(),
        "token"
            | "bottoken"
            | "telegramtoken"
            | "accesstoken"
            | "refreshtoken"
            | "tokenencrypted"
            | "apikey"
            | "xapikey"
            | "apikeyencrypted"
            | "secret"
            | "clientsecret"
            | "secretencrypted"
            | "password"
            | "passphrase"
            | "authorization"
            | "pin"
    )
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let hay = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || from > hay.len() || hay.len() - from < needle.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| {
        hay[i..i + needle.len()].eq_ignore_ascii_case(needle)
    })
}

fn mask_after_key(input: &str, key: &str) -> String {
    let mut result = input.to_string();
    let mut search_from = 0usize;
    loop {
        let Some(key_pos) = find_ascii_case_insensitive(&result, key, search_from) else {
            break;
        };
        let bytes = result.as_bytes();
        let mut pos = key_pos + key.len();
        // Lewati spasi dan kutip penutup key JSON (`"token": ...`).
        while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
            pos += 1;
        }
        if pos < bytes.len() && matches!(bytes[pos], b'"' | b'\'') {
            pos += 1;
            while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
                pos += 1;
            }
        }
        // Hanya mask bila key diikuti separator `=` atau `:` agar kata
        // biasa (mis. "token tidak valid") tidak ikut termask.
        if pos >= bytes.len() || !matches!(bytes[pos], b'=' | b':') {
            search_from = key_pos + key.len();
            continue;
        }
        pos += 1;
        while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t' | b'"' | b'\'') {
            pos += 1;
        }
        let value_start = pos;
        let rest = &result[value_start..];
        let value_len = rest
            .char_indices()
            .find(|(_, c)| {
                c.is_whitespace()
                    || matches!(c, '"' | '\'' | ',' | '}' | ']' | '&' | ';' | ')' | '<' | '>')
            })
            .map(|(idx, _)| idx)
            .unwrap_or(rest.len());
        if value_len > 0 && &rest[..value_len] != REDACTED {
            result.replace_range(value_start..value_start + value_len, REDACTED);
            search_from = value_start + REDACTED.len();
        } else {
            search_from = key_pos + key.len();
        }
    }
    result
}

fn redact_bot_token_pattern(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            // Token bot Telegram: id numerik (≥5 digit) + ':' + secret
            // panjang (≥20 karakter alfanumerik/_/-).
            if j - i >= 5 && j < chars.len() && chars[j] == ':' {
                let mut k = j + 1;
                while k < chars.len()
                    && (chars[k].is_ascii_alphanumeric() || chars[k] == '_' || chars[k] == '-')
                {
                    k += 1;
                }
                if k - (j + 1) >= 20 {
                    out.push_str(REDACTED);
                    i = k;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_token() -> String {
        // Token sintetis yang jelas palsu, dibangun saat runtime agar
        // tidak ada token utuh tertulis di source.
        format!("123456789:{}", "FAKE".repeat(9))
    }

    #[test]
    fn redacts_bot_token_in_url() {
        let token = synthetic_token();
        let input = format!(
            "error sending request for url (https://api.telegram.org/bot{token}/getMe): connection failed"
        );
        let out = redact_secrets(&input);
        assert!(!out.contains(&token));
        assert!(out.contains("/bot****/getMe"));
    }

    #[test]
    fn redacts_known_short_secret_exactly() {
        let secret = "123456:SHORTFAKE";
        let out = redact_known_secret(&format!("gagal untuk bot {secret}"), secret);
        assert!(!out.contains(secret));
        assert!(out.contains("****"));
    }

    #[test]
    fn redacts_key_values_and_json_keys() {
        let out = redact_secrets("token=abc123 dan \"api_key\": \"xyz789\" selesai");
        assert!(!out.contains("abc123"));
        assert!(!out.contains("xyz789"));
        // Kata biasa tanpa separator tidak ikut termask.
        assert_eq!(redact_secrets("token tidak valid"), "token tidak valid");
    }

    #[test]
    fn redacts_json_recursively_and_sensitive_keys() {
        let token = synthetic_token();
        let mut value = serde_json::json!({
            "error": format!("url https://api.telegram.org/bot{token}/getMe gagal"),
            "token": "nilai-pendek",
            "token_masked": "****FAKE",
            "nested": [{"note": format!("bot {token}")}]
        });
        redact_json_value(&mut value);
        let text = value.to_string();
        assert!(!text.contains(&token));
        assert!(!text.contains("nilai-pendek"));
        // token_masked yang sudah aman tetap tampil.
        assert_eq!(value["token_masked"], "****FAKE");
    }

    #[test]
    fn redact_json_text_falls_back_to_plain_text() {
        let token = synthetic_token();
        let out = redact_json_text(&format!("bukan json {token}"));
        assert!(!out.contains(&token));
    }
}
