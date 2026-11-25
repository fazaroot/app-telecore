//! State aplikasi yang dibagikan ke semua handler (Clone, berbasis Arc).

use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::automation::AutomationGuard;
use crate::bot::telegram::TelegramClient;
use crate::bot::PollingManager;
use crate::config::Config;
use crate::middleware::RateLimiter;
use crate::security::Crypto;
use crate::websocket::Hub;

/// Pelacak flood sederhana: jumlah pesan per (bot, chat, user) dalam 60 detik terakhir.
#[derive(Default)]
pub struct FloodTracker {
    inner: Mutex<HashMap<String, Vec<Instant>>>,
}

impl FloodTracker {
    /// Catat satu pesan, kembalikan jumlah pesan dalam 60 detik terakhir.
    pub fn record(&self, bot_id: &str, chat_id: i64, user_id: i64) -> usize {
        let key = format!("{bot_id}:{chat_id}:{user_id}");
        let mut map = self.inner.lock().unwrap();
        let now = Instant::now();
        let entry = map.entry(key).or_default();
        entry.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        entry.push(now);
        entry.len()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Config,
    pub crypto: Arc<Crypto>,
    pub telegram: TelegramClient,
    pub hub: Hub,
    pub polling: PollingManager,
    pub rate_limiter: RateLimiter,
    pub automation_guard: Arc<AutomationGuard>,
    pub flood: Arc<FloodTracker>,
    pub started_at: Instant,
}

impl AppState {
    pub fn new(
        pool: SqlitePool,
        config: Config,
        crypto: Arc<Crypto>,
        telegram: TelegramClient,
    ) -> Self {
        Self {
            pool,
            config,
            crypto,
            telegram,
            hub: Hub::new(),
            polling: PollingManager::new(),
            rate_limiter: RateLimiter::new(120), // 120 request/menit per IP
            automation_guard: Arc::new(AutomationGuard::new()),
            flood: Arc::new(FloodTracker::default()),
            started_at: Instant::now(),
        }
    }
}
