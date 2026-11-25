//! Logika bisnis bersama: pemrosesan update Telegram + activity log.

pub mod update_processor;

pub use update_processor::{log_activity, process_update};
