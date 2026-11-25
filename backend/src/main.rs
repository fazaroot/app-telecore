//! TeleCore backend — Telegram Bot Control Center.
//!
//! Server binary Axum (REST + WebSocket) + SQLite. Berjalan di server/PC/Termux,
//! BUKAN di dalam APK. APK hanya berisi frontend yang terhubung ke server ini
//! lewat URL yang bisa dikonfigurasi.

mod api;
mod automation;
mod bot;
mod config;
mod db;
mod error;
mod logging;
mod middleware;
mod models;
mod scheduler;
mod security;
mod services;
mod state;
mod websocket;

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use bot::telegram::TelegramClient;
use config::Config;
use logging::LogLayer;
use security::Crypto;
use state::AppState;
use websocket::Hub;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;

    // Infrastruktur live logs (v1.1) dibuat SEBELUM subscriber tracing,
    // agar layer log memakai handle yang sama dengan AppState.
    let (log_tx, _log_rx) = broadcast::channel(200);
    let log_ring = Arc::new(Mutex::new(VecDeque::new()));
    let hub = Hub::new();
    let log_layer = LogLayer::new(log_tx.clone(), log_ring.clone(), hub.clone());

    // Logging: filter dari RUST_LOG (default info). Layer fmt tetap jalan;
    // LogLayer hanya menambah sink (ring buffer + broadcast + WS hub).
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(&config.rust_log))
        .with(tracing_subscriber::fmt::layer())
        .with(log_layer)
        .init();

    let crypto = Arc::new(Crypto::load_or_generate()?);

    // Terapkan restore backup yang tertunda SEBELUM pool dibuka.
    db::apply_pending_restore(&config.db_path())?;

    let pool = db::init_pool(&config.database_url).await?;
    tracing::info!("Database siap: {}", config.db_path());

    let telegram = TelegramClient::new()?;
    let state = AppState::new_with_log(
        pool,
        config.clone(),
        crypto,
        telegram,
        hub,
        log_tx,
        log_ring,
    );

    // Scheduler: loop tiap 10 detik.
    tokio::spawn(scheduler::scheduler_loop(state.clone()));

    // Long polling untuk bot aktif (polling_enabled, bukan webhook).
    bot::start_polling_for_active_bots(&state).await;

    let app = api::build_router(state.clone());
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("TeleCore backend berjalan di http://{addr} (health: /api/health)");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(state.clone()))
    .await?;

    // Hentikan semua task polling saat shutdown.
    state.polling.stop_all().await;
    tracing::info!("TeleCore backend berhenti.");
    Ok(())
}

async fn shutdown_signal(state: AppState) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("gagal memasang handler Ctrl+C");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("gagal memasang handler SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Sinyal shutdown diterima, menghentikan server...");
    let _ = state;
}
