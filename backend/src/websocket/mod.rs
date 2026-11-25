//! WebSocket broadcast hub: semua event server dipublikasikan sebagai JSON
//! `{"type": "...", "payload": {...}, "ts": "..."}` ke semua klien /ws yang terhubung.

pub mod hub;

pub use hub::{ws_handler, Hub};
