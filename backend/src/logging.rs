//! Layer tracing kustom (v1.1): setiap event tracing juga ditulis sebagai
//! baris teks ke ring buffer (200 baris terakhir, GET /api/logs), ke
//! broadcast channel, dan dipublikasikan ke WebSocket hub sebagai event
//! tipe `"log"`. Layer `fmt` yang sudah ada tetap jalan — layer ini hanya
//! menambah sink, tidak menggantikan logging yang sudah ada.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use chrono::{SecondsFormat, Utc};
use tokio::sync::broadcast;
use tracing::{Event, Subscriber};
use tracing_subscriber::{layer::Context, Layer};

use crate::state::LOG_BUFFER_CAPACITY;
use crate::websocket::Hub;

/// Layer yang menyalin event tracing ke ring buffer + broadcast + WS hub.
pub struct LogLayer {
    tx: broadcast::Sender<String>,
    ring: Arc<Mutex<VecDeque<String>>>,
    hub: Hub,
}

impl LogLayer {
    pub fn new(
        tx: broadcast::Sender<String>,
        ring: Arc<Mutex<VecDeque<String>>>,
        hub: Hub,
    ) -> Self {
        Self { tx, ring, hub }
    }

    /// Format satu event menjadi satu baris log. Pure (kecuali timestamp),
    /// dipakai juga oleh unit test.
    pub fn format_line(level: &str, message: &str, fields: &[(String, String)]) -> String {
        let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut line = format!("{ts} [{level}] {message}");
        for (k, v) in fields {
            let _ = write!(line, " {k}={v}");
        }
        line
    }

    fn push_line(&self, line: String, level: &str, message: &str) {
        {
            let mut ring = self.ring.lock().unwrap();
            if ring.len() >= LOG_BUFFER_CAPACITY {
                ring.pop_front();
            }
            ring.push_back(line.clone());
        }
        let _ = self.tx.send(line.clone());
        self.hub.publish(
            "log",
            serde_json::json!({"line": line, "level": level, "message": message}),
        );
    }
}

struct FieldVisitor {
    message: Option<String>,
    fields: Vec<(String, String)>,
}

impl tracing::field::Visit for FieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.push((field.name().to_string(), value.to_string()));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let text = format!("{value:?}");
        if field.name() == "message" {
            // Pesan string yang terekam via Debug membawa tanda kutip —
            // kupas satu lapis kutip bila ada.
            let unquoted = text
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(&text)
                .to_string();
            self.message = Some(unquoted);
        } else {
            self.fields.push((field.name().to_string(), text));
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields.push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields.push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields.push((field.name().to_string(), value.to_string()));
    }
}

impl<S: Subscriber> Layer<S> for LogLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let level = meta.level().to_string();
        let mut visitor = FieldVisitor {
            message: None,
            fields: Vec::new(),
        };
        event.record(&mut visitor);
        // Redaksi sebelum masuk ring buffer/broadcast/WebSocket: log
        // tidak boleh mengandung token bot atau secret lain.
        let message = crate::security::redact::redact_secrets(&visitor.message.unwrap_or_default());
        let fields: Vec<(String, String)> = visitor
            .fields
            .into_iter()
            .map(|(k, v)| (k, crate::security::redact::redact_secrets(&v)))
            .collect();
        // Abaikan event kosong (mis. dari span tanpa pesan) agar ring tidak
        // dipenuhi baris tanpa isi.
        if message.is_empty() && fields.is_empty() {
            return;
        }
        let line = Self::format_line(&level, &message, &fields);
        self.push_line(line, &level, &message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_line_contains_timestamp_level_message() {
        let line = LogLayer::format_line("INFO", "server jalan", &[("port".to_string(), "8080".to_string())]);
        assert!(line.contains("[INFO]"));
        assert!(line.contains("server jalan"));
        assert!(line.contains("port=8080"));
        assert!(line.contains('T')); // timestamp RFC3339
    }
}
