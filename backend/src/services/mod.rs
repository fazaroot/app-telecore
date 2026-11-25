//! Logika bisnis bersama: pemrosesan update Telegram + activity log.

pub mod outgoing_webhooks;
pub mod template_vars;
pub mod tokens;
pub mod update_processor;

pub use template_vars::{lookup_template_vars, render_template_vars, TemplateVars};
pub use update_processor::{log_activity, process_update};
