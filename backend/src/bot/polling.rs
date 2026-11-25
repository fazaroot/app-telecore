//! Long polling per bot aktif sebagai tokio task.
//!
//! - `getUpdates` timeout 30 detik, offset tersimpan di `bot_configurations`.
//! - Long polling dan webhook TIDAK boleh aktif bersamaan untuk bot yang sama:
//!   mengaktifkan webhook menghentikan task polling bot itu, dan sebaliknya
//!   (lihat `api::bots`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

use crate::services::process_update;
use crate::state::AppState;
use crate::{bot::get_bot_token, middleware::sanitize_log};

#[derive(Clone)]
pub struct PollingManager {
    tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl PollingManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Jalankan task polling untuk satu bot (hentikan dulu bila sudah berjalan).
    pub async fn start(&self, state: AppState, bot_id: &str) {
        self.stop(bot_id).await;
        let bot_id_owned = bot_id.to_string();
        let handle = tokio::spawn(polling_loop(state, bot_id_owned.clone()));
        self.tasks.lock().unwrap().insert(bot_id_owned, handle);
    }

    /// Hentikan task polling satu bot (bila ada).
    pub async fn stop(&self, bot_id: &str) {
        // Ambil handle dulu, lepas lock SEBELUM await: MutexGuard std
        // bersifat !Send dan tidak boleh tertahan melewati titik await
        // (akan membuat future pemanggil !Send dan ditolak axum).
        let handle = self.tasks.lock().unwrap().remove(bot_id);
        if let Some(handle) = handle {
            handle.abort();
            let _ = handle.await;
        }
    }

    /// Apakah task polling bot sedang berjalan (API publik untuk diagnostik).
    #[allow(dead_code)]
    pub async fn is_running(&self, bot_id: &str) -> bool {
        self.tasks.lock().unwrap().contains_key(bot_id)
    }

    pub async fn stop_all(&self) {
        let handles: Vec<JoinHandle<()>> =
            self.tasks.lock().unwrap().drain().map(|(_, h)| h).collect();
        for h in handles {
            h.abort();
        }
    }
}

impl Default for PollingManager {
    fn default() -> Self {
        Self::new()
    }
}

async fn polling_loop(state: AppState, bot_id: String) {
    tracing::info!("Polling dimulai untuk bot {bot_id}");
    loop {
        // Ambil token; bila bot dihapus / token rusak, tunggu lalu coba lagi.
        let token = match get_bot_token(&state, &bot_id).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("Polling bot {bot_id}: {e}");
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };
        let offset = get_offset(&state, &bot_id).await;
        match state.telegram.get_updates(&token, offset, 30).await {
            Ok(updates) => {
                for update in updates {
                    let new_offset = update.update_id + 1;
                    process_update(&state, &bot_id, &update).await;
                    set_offset(&state, &bot_id, new_offset).await;
                }
            }
            Err(e) => {
                // Jangan log token: pesan error Telegram tidak mengandung token,
                // tapi tetap disanitasi untuk jaga-jaga.
                tracing::warn!("Polling bot {bot_id} error: {}", sanitize_log(&e.to_string()));
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn get_offset(state: &AppState, bot_id: &str) -> Option<i64> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT last_update_offset FROM bot_configurations WHERE bot_id = ?")
            .bind(bot_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    row.map(|(o,)| if o == 0 { None } else { Some(o) })
        .unwrap_or(None)
}

async fn set_offset(state: &AppState, bot_id: &str, offset: i64) {
    let _ = sqlx::query("UPDATE bot_configurations SET last_update_offset = ? WHERE bot_id = ?")
        .bind(offset)
        .bind(bot_id)
        .execute(&state.pool)
        .await;
}
