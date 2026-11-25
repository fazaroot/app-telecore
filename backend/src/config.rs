//! Konfigurasi server dari environment variable.

use std::env;

/// Konfigurasi runtime TeleCore backend.
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    /// API key untuk header `X-API-Key`. Kosong = semua /api/* ditolak (401).
    pub api_key: String,
    pub rust_log: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let host = env::var("TELECORE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port: u16 = env::var("TELECORE_PORT")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(8080);
        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:telecore.db".to_string());
        let api_key = env::var("TELECORE_API_KEY").unwrap_or_default();
        if api_key.is_empty() {
            tracing::warn!(
                "TELECORE_API_KEY tidak diset — semua endpoint /api/* akan menolak request (401). \
                 Set env ini untuk mengaktifkan API."
            );
        }
        let rust_log = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
        Ok(Self {
            host,
            port,
            database_url,
            api_key,
            rust_log,
        })
    }

    /// Path file SQLite dari DATABASE_URL (`sqlite:path` / `sqlite://path` / `sqlite::memory:`).
    pub fn db_path(&self) -> String {
        db_path_from_url(&self.database_url)
    }
}

/// Ambil path file dari URL SQLite.
pub fn db_path_from_url(url: &str) -> String {
    let s = url.strip_prefix("sqlite:").unwrap_or(url);
    let s = s.strip_prefix("//").unwrap_or(s);
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_path_parsing() {
        assert_eq!(db_path_from_url("sqlite:telecore.db"), "telecore.db");
        assert_eq!(db_path_from_url("sqlite:/tmp/x.db"), "/tmp/x.db");
        assert_eq!(db_path_from_url("sqlite://telecore.db"), "telecore.db");
        assert_eq!(db_path_from_url("sqlite::memory:"), ":memory:");
    }
}
