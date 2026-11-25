//! State aplikasi yang dibagikan ke semua handler (Clone, berbasis Arc).

use sqlx::SqlitePool;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use crate::automation::AutomationGuard;
use crate::bot::telegram::TelegramClient;
use crate::bot::PollingManager;
use crate::config::Config;
use crate::middleware::RateLimiter;
use crate::security::Crypto;
use crate::websocket::Hub;

/// Kapasitas ring buffer log terakhir + channel broadcast log.
pub const LOG_BUFFER_CAPACITY: usize = 200;

// ---------------------------------------------------------------------------
// Broadcast pesan massal (v1.1): status disimpan in-memory saja.
// Hilang saat server restart — didokumentasikan di docs/MCP.md.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BroadcastRecipientResult {
    pub chat_id: i64,
    pub status: String, // sent | failed
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BroadcastStatus {
    pub broadcast_id: String,
    pub bot_id: String,
    pub total: i64,
    pub sent: i64,
    pub failed: i64,
    /// running | done
    pub status: String,
    pub results: Vec<BroadcastRecipientResult>,
    pub created_at: String,
}

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
    /// Channel broadcast baris log (kapasitas 200) — dipakai layer tracing.
    pub log_tx: broadcast::Sender<String>,
    /// Ring buffer 200 baris log terakhir (GET /api/logs).
    pub log_ring: Arc<Mutex<VecDeque<String>>>,
    /// Registry status broadcast (in-memory; hilang saat restart).
    pub broadcasts: Arc<Mutex<HashMap<String, BroadcastStatus>>>,
}

impl AppState {
    pub fn new(
        pool: SqlitePool,
        config: Config,
        crypto: Arc<Crypto>,
        telegram: TelegramClient,
    ) -> Self {
        let (log_tx, _) = broadcast::channel(LOG_BUFFER_CAPACITY);
        Self::new_with_log(
            pool,
            config,
            crypto,
            telegram,
            Hub::new(),
            log_tx,
            Arc::new(Mutex::new(VecDeque::new())),
        )
    }

    /// Konstruktor dengan infrastruktur log + hub yang sudah dibuat di luar
    /// (dipakai `main.rs` agar layer tracing bisa memakai handle yang sama).
    pub fn new_with_log(
        pool: SqlitePool,
        config: Config,
        crypto: Arc<Crypto>,
        telegram: TelegramClient,
        hub: Hub,
        log_tx: broadcast::Sender<String>,
        log_ring: Arc<Mutex<VecDeque<String>>>,
    ) -> Self {
        Self {
            pool,
            config,
            crypto,
            telegram,
            hub,
            polling: PollingManager::new(),
            rate_limiter: RateLimiter::new(120), // 120 request/menit per IP
            automation_guard: Arc::new(AutomationGuard::new()),
            flood: Arc::new(FloodTracker::default()),
            started_at: Instant::now(),
            log_tx,
            log_ring,
            broadcasts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Ambil N baris log terakhir (paling lama dulu).
    pub fn recent_logs(&self, limit: usize) -> Vec<String> {
        let ring = self.log_ring.lock().unwrap();
        let n = limit.min(ring.len());
        ring.iter().skip(ring.len() - n).cloned().collect()
    }

    /// Subscribe ke baris log baru (channel broadcast, kapasitas 200).
    pub fn subscribe_logs(&self) -> broadcast::Receiver<String> {
        self.log_tx.subscribe()
    }
}
