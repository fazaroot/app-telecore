use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;

use crate::models::now_rfc3339;
use crate::state::AppState;

#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<String>,
}

impl Hub {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    /// Publikasikan event ke semua klien WebSocket yang terhubung.
    /// `event_type`: "activity" | "bot_status" | "message" | "notification" | "stats".
    pub fn publish(&self, event_type: &str, payload: serde_json::Value) {
        let event = serde_json::json!({
            "type": event_type,
            "payload": payload,
            "ts": now_rfc3339(),
        });
        let _ = self.tx.send(event.to_string());
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

/// Handler `GET /ws` — upgrade ke WebSocket, lalu teruskan semua event hub.
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.hub.subscribe();

    // Pesan sambutan agar klien tahu koneksi hidup.
    let welcome = serde_json::json!({
        "type": "activity",
        "payload": {"message": "Terhubung ke TeleCore WebSocket"},
        "ts": now_rfc3339(),
    });
    if sender.send(Message::Text(welcome.to_string())).await.is_err() {
        return;
    }

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Drain pesan masuk dari klien (ping/pong ditangani otomatis oleh axum);
    // tutup koneksi saat klien disconnect.
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) => {}
            Ok(_) => {}
            Err(_) => break,
        }
    }
    send_task.abort();
    let _ = send_task.await;
}
