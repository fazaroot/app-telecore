//! Enkripsi token bot dengan AES-256-GCM.
//!
//! - Kunci dari env `TELECORE_ENCRYPTION_KEY`: hex 64 karakter ATAU base64 44
//!   karakter (32 byte). Jika env tidak ada, kunci digenerate sekali saat
//!   pertama jalan dan disimpan ke file `telecore.key` mode 0600.
//! - Format ciphertext: base64(nonce 12 byte || ciphertext+tag).
//! - Token TIDAK PERNAH dikembalikan lengkap via API; gunakan
//!   [`Crypto::mask_token`] untuk tampilan (hanya 4 karakter terakhir).

use aes_gcm::{
    aead::{Aead, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rand::RngCore;
use std::fs;
use std::os::unix::fs::PermissionsExt;

const KEY_FILE: &str = "telecore.key";

pub struct Crypto {
    cipher: Aes256Gcm,
}

impl Crypto {
    pub fn from_key_bytes(key: [u8; 32]) -> Self {
        Self {
            cipher: Aes256Gcm::new(&key.into()),
        }
    }

    /// Muat kunci dari env, atau generate + simpan ke `telecore.key` bila tidak ada.
    pub fn load_or_generate() -> anyhow::Result<Self> {
        if let Ok(raw) = std::env::var("TELECORE_ENCRYPTION_KEY") {
            let raw = raw.trim();
            if !raw.is_empty() {
                return Self::from_env_value(raw);
            }
        }
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        let hex_key = hex::encode(key);
        fs::write(KEY_FILE, &hex_key)?;
        fs::set_permissions(KEY_FILE, fs::Permissions::from_mode(0o600))?;
        tracing::warn!(
            "TELECORE_ENCRYPTION_KEY tidak diset — kunci baru digenerate dan disimpan ke \
             telecore.key (mode 0600). BACKUP file ini; tanpanya token bot terenkripsi \
             tidak bisa didekripsi lagi."
        );
        Ok(Self::from_key_bytes(key))
    }

    fn from_env_value(raw: &str) -> anyhow::Result<Self> {
        // Coba hex 64 karakter dulu.
        if raw.len() == 64 {
            if let Ok(bytes) = hex::decode(raw) {
                if bytes.len() == 32 {
                    let key: [u8; 32] = bytes.try_into().unwrap();
                    return Ok(Self::from_key_bytes(key));
                }
            }
        }
        // Lalu base64 (44 karakter untuk 32 byte).
        if let Ok(bytes) = BASE64.decode(raw) {
            if bytes.len() == 32 {
                let key: [u8; 32] = bytes.try_into().unwrap();
                return Ok(Self::from_key_bytes(key));
            }
        }
        anyhow::bail!(
            "TELECORE_ENCRYPTION_KEY tidak valid: harus hex 64 karakter atau base64 44 karakter (32 byte)"
        )
    }

    pub fn encrypt(&self, plaintext: &str) -> anyhow::Result<String> {
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("enkripsi gagal: {e}"))?;
        let mut out = nonce_bytes.to_vec();
        out.extend_from_slice(&ciphertext);
        Ok(BASE64.encode(out))
    }

    pub fn decrypt(&self, encoded: &str) -> anyhow::Result<String> {
        let bytes = BASE64.decode(encoded)?;
        if bytes.len() < 12 {
            anyhow::bail!("ciphertext terlalu pendek");
        }
        let (nonce_bytes, ciphertext) = bytes.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plain = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("dekripsi gagal (kunci salah atau data rusak): {e}"))?;
        Ok(String::from_utf8(plain)?)
    }

    /// Masking token: hanya 4 karakter terakhir yang tampil, mis. "****abcd".
    pub fn mask_token(token: &str) -> String {
        let tail: String = token.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        if token.chars().count() <= 4 {
            "****".to_string()
        } else {
            format!("****{tail}")
        }
    }
}

/// Hash PIN admin: SHA-256(salt || pin), salt berupa hex acak 16 byte.
/// Disimpan sebagai (hash_hex, salt_hex) di `app_settings`.
pub fn hash_pin(pin: &str, salt_hex: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(salt_hex.as_bytes());
    hasher.update(pin.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn generate_salt_hex() -> String {
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    hex::encode(salt)
}

/// Enkripsi backup dengan passphrase: kunci = SHA-256(passphrase).
/// Format sama seperti Crypto: base64(nonce || ciphertext).
pub fn encrypt_with_passphrase(passphrase: &str, plaintext: &[u8]) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let key = Sha256::digest(passphrase.as_bytes());
    let crypto = Crypto::from_key_bytes(key.into());
    // Reuse logika encrypt dengan input biner melalui helper string tidak aman
    // untuk data biner, jadi enkripsi di sini langsung.
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = crypto
        .cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("enkripsi backup gagal: {e}"))?;
    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&ciphertext);
    Ok(BASE64.encode(out))
}

pub fn decrypt_with_passphrase(passphrase: &str, encoded: &str) -> anyhow::Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    let key = Sha256::digest(passphrase.as_bytes());
    let crypto = Crypto::from_key_bytes(key.into());
    let bytes = BASE64.decode(encoded)?;
    if bytes.len() < 12 {
        anyhow::bail!("ciphertext backup terlalu pendek");
    }
    let (nonce_bytes, ciphertext) = bytes.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plain = crypto
        .cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("dekripsi backup gagal (passphrase salah atau file rusak): {e}"))?;
    Ok(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trip() {
        let crypto = Crypto::from_key_bytes([7u8; 32]);
        let token = "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11";
        let enc = crypto.encrypt(token).unwrap();
        assert_ne!(enc, token);
        assert_eq!(crypto.decrypt(&enc).unwrap(), token);
        // Nonce acak: dua enkripsi plaintext sama menghasilkan ciphertext beda.
        assert_ne!(crypto.encrypt(token).unwrap(), enc);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let a = Crypto::from_key_bytes([1u8; 32]);
        let b = Crypto::from_key_bytes([2u8; 32]);
        let enc = a.encrypt("secret").unwrap();
        assert!(b.decrypt(&enc).is_err());
    }

    #[test]
    fn mask_token_shows_only_last_four() {
        assert_eq!(Crypto::mask_token("123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11"), "****ew11");
        assert_eq!(Crypto::mask_token("abcd"), "****");
        assert_eq!(Crypto::mask_token(""), "****");
    }

    #[test]
    fn passphrase_round_trip() {
        let data = b"SQLite format 3\0-- fake db bytes";
        let enc = encrypt_with_passphrase("passphrase-rahasia", data).unwrap();
        let dec = decrypt_with_passphrase("passphrase-rahasia", &enc).unwrap();
        assert_eq!(dec, data);
        assert!(decrypt_with_passphrase("passphrase-salah", &enc).is_err());
    }

    #[test]
    fn pin_hash_is_deterministic_with_same_salt() {
        let salt = "00112233445566778899aabbccddeeff";
        assert_eq!(hash_pin("1234", salt), hash_pin("1234", salt));
        assert_ne!(hash_pin("1234", salt), hash_pin("4321", salt));
    }
}
