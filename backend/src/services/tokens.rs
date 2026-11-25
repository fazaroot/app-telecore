//! Personal API tokens (v1.1).
//!
//! Format token: `"tc_" + 32 karakter hex acak` (16 byte acak).
//! Yang disimpan di DB hanya SHA-256 hex dari token penuh + prefix
//! (8 karakter pertama token, mis. `tc_ab12cd`). Token penuh hanya
//! ditampilkan SEKALI saat pembuatan — setelah itu tidak bisa diambil lagi.

use rand::RngCore;
use sha2::{Digest, Sha256};

pub const TOKEN_PREFIX_LEN: usize = 8;

/// Buat token personal baru. Mengembalikan (token_penuh, prefix, token_hash).
pub fn generate_personal_token() -> (String, String, String) {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = format!("tc_{}", hex::encode(bytes));
    let prefix = token.chars().take(TOKEN_PREFIX_LEN).collect();
    let hash = hash_token(&token);
    (token, prefix, hash)
}

/// SHA-256 hex dari token penuh. Pure — diuji unit.
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Prefix = 8 karakter pertama token. Pure — diuji unit.
pub fn token_prefix(token: &str) -> String {
    token.chars().take(TOKEN_PREFIX_LEN).collect()
}

/// Validasi format token personal (tanpa DB): harus `tc_` + 32 hex.
pub fn is_valid_token_format(token: &str) -> bool {
    let rest = match token.strip_prefix("tc_") {
        Some(r) => r,
        None => return false,
    };
    rest.len() == 32 && rest.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_token_has_expected_format() {
        let (token, prefix, hash) = generate_personal_token();
        assert!(is_valid_token_format(&token));
        assert_eq!(prefix.len(), 8);
        assert_eq!(prefix, token_prefix(&token));
        assert!(prefix.starts_with("tc_"));
        assert_eq!(hash, hash_token(&token));
        assert_eq!(hash.len(), 64);
        // Token penuh TIDAK PERNAH sama dengan hash-nya.
        assert_ne!(token, hash);
    }

    #[test]
    fn two_generated_tokens_differ() {
        let (a, _, _) = generate_personal_token();
        let (b, _, _) = generate_personal_token();
        assert_ne!(a, b);
    }

    #[test]
    fn hashing_is_deterministic_and_format_checked() {
        assert_eq!(hash_token("tc_abc"), hash_token("tc_abc"));
        assert_ne!(hash_token("tc_abc"), hash_token("tc_abd"));
        assert!(is_valid_token_format(&format!("tc_{}", "a".repeat(32))));
        assert!(!is_valid_token_format("tc_abc")); // terlalu pendek
        assert!(!is_valid_token_format("xx_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(!is_valid_token_format("tc_gggggggggggggggggggggggggggggggg")); // bukan hex
        assert_eq!(token_prefix("tc_abcdef1234"), "tc_abcde");
    }
}
