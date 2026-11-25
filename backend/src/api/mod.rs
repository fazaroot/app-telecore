//! Router HTTP: semua endpoint REST + WebSocket.
//!
//! - `/api/health` dan `/api/webhooks/telegram/*` TANPA auth.
//! - Semua `/api/*` lainnya WAJIB header `X-API-Key` (middleware auth).
//! - Rate limit per IP berlaku untuk semua route.

pub mod analytics;
pub mod automation;
pub mod backups;
pub mod bots;
pub mod commands;
pub mod groups;
pub mod health;
pub mod logs;
pub mod mcp;
pub mod messages;
pub mod notifications;
pub mod outgoing_webhooks;
pub mod scheduler;
pub mod security;
pub mod settings;
pub mod users;
pub mod webhooks;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use tower_http::cors::CorsLayer;

use crate::middleware::{auth_middleware, rate_limit_middleware};
use crate::state::AppState;
use crate::websocket;

pub fn build_router(state: AppState) -> Router<()> {
    let api = Router::new()
        // Bots
        .route("/bots", get(bots::list_bots).post(bots::create_bot))
        .route("/bots/export", get(bots::export_bots))
        .route("/bots/import", post(bots::import_bots))
        .route(
            "/bots/:id",
            get(bots::get_bot).put(bots::update_bot).delete(bots::delete_bot),
        )
        .route("/bots/:id/connect", post(bots::connect_bot))
        .route("/bots/:id/disconnect", post(bots::disconnect_bot))
        .route("/bots/:id/test", post(bots::test_bot))
        .route(
            "/bots/:id/config",
            get(bots::get_config).put(bots::update_config),
        )
        .route("/bots/:id/webhook", post(bots::set_bot_webhook))
        // Messages
        // POST /messages = kirim (dipakai frontend); /messages/send alias lama.
        .route(
            "/messages",
            get(messages::list_messages).post(messages::send_message),
        )
        .route("/messages/send", post(messages::send_message))
        .route("/messages/search", get(messages::search_messages))
        .route("/messages/preview", post(messages::preview_template))
        .route("/messages/broadcast", post(messages::create_broadcast))
        .route(
            "/messages/broadcast/:id",
            get(messages::get_broadcast_status),
        )
        .route("/messages/send-media", post(messages::send_media_multipart))
        .route("/messages/history", get(messages::message_history))
        .route("/messages/:id/retry", post(messages::retry_message))
        // Commands
        .route(
            "/commands",
            get(commands::list_commands).post(commands::create_command),
        )
        .route(
            "/commands/:id",
            get(commands::get_command)
                .put(commands::update_command)
                .delete(commands::delete_command),
        )
        .route("/commands/:id/test", post(commands::test_command))
        // Users
        .route("/users", get(users::list_users))
        .route("/users/export", get(users::export_users))
        .route("/users/:id", get(users::get_user).put(users::update_user))
        .route("/users/:id/block", post(users::block_user))
        .route("/users/:id/unblock", post(users::unblock_user))
        // Groups
        .route("/groups", get(groups::list_groups))
        .route(
            "/groups/:id",
            get(groups::get_group).put(groups::update_group_settings),
        )
        .route("/groups/:id/settings", put(groups::update_group_settings))
        .route("/groups/:id/moderation-logs", get(groups::moderation_logs))
        .route("/groups/:id/moderate", post(groups::moderate))
        // Automation
        .route(
            "/automation",
            get(automation::list_workflows).post(automation::create_workflow),
        )
        .route("/automation/executions", get(automation::list_executions))
        .route(
            "/automation/:id/executions",
            get(automation::list_workflow_executions),
        )
        .route(
            "/automation/:id",
            get(automation::get_workflow)
                .put(automation::update_workflow)
                .delete(automation::delete_workflow),
        )
        .route("/automation/:id/toggle", post(automation::toggle_workflow))
        // Scheduler
        .route(
            "/scheduler",
            get(scheduler::list_tasks).post(scheduler::create_task),
        )
        .route("/scheduler/history", get(scheduler::task_history))
        .route(
            "/scheduler/:id/history",
            get(scheduler::task_history_for_task),
        )
        .route(
            "/scheduler/:id",
            get(scheduler::get_task)
                .put(scheduler::update_task)
                .delete(scheduler::delete_task),
        )
        .route("/scheduler/:id/pause", post(scheduler::pause_task))
        .route("/scheduler/:id/resume", post(scheduler::resume_task))
        // Analytics
        .route("/analytics/summary", get(analytics::summary))
        .route("/analytics/export", get(analytics::export))
        .route("/analytics/per-bot", get(analytics::per_bot))
        .route("/analytics/per-bot/export", get(analytics::per_bot_export))
        // Live logs (v1.1)
        .route("/logs", get(logs::list_logs))
        // Outgoing webhooks (v1.1)
        .route(
            "/outgoing-webhooks",
            get(outgoing_webhooks::list_webhooks).post(outgoing_webhooks::create_webhook),
        )
        .route(
            "/outgoing-webhooks/:id",
            get(outgoing_webhooks::get_webhook)
                .put(outgoing_webhooks::update_webhook)
                .delete(outgoing_webhooks::delete_webhook),
        )
        .route(
            "/outgoing-webhooks/:id/deliveries",
            get(outgoing_webhooks::list_deliveries),
        )
        .route(
            "/outgoing-webhooks/:id/test",
            post(outgoing_webhooks::test_webhook),
        )
        .route(
            "/outgoing-webhooks/:id/deliveries/:delivery_id/retry",
            post(outgoing_webhooks::retry_delivery),
        )
        // Notifications
        .route("/notifications", get(notifications::list_notifications))
        .route("/notifications/read-all", post(notifications::mark_all_read))
        .route("/notifications/:id/read", post(notifications::mark_read))
        .route(
            "/notifications/:id",
            delete(notifications::delete_notification),
        )
        // Security
        .route("/security/status", get(security::status))
        .route("/security/change-pin", post(security::change_pin))
        .route("/security/lock", post(security::lock))
        .route("/security/unlock", post(security::unlock))
        .route("/security/sessions", get(security::list_sessions))
        // Personal API tokens (v1.1)
        .route(
            "/security/tokens",
            get(security::list_tokens).post(security::create_token),
        )
        .route("/security/tokens/:id", delete(security::delete_token))
        // Backups
        .route(
            "/backups",
            get(backups::list_backups).post(backups::create_backup),
        )
        .route("/backups/:id/restore", post(backups::restore_backup))
        .route("/backups/:id", delete(backups::delete_backup))
        // Settings
        .route(
            "/settings",
            get(settings::get_settings).put(settings::update_settings),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // MCP server (v1.1): POST /mcp di root, auth X-API-Key yang sama
    // dengan API (env key atau personal token).
    let mcp = Router::new()
        .route("/mcp", post(mcp::mcp_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .route("/api/health", get(health::health))
        .route(
            "/api/webhooks/telegram/:bot_id",
            post(webhooks::telegram_webhook),
        )
        .route("/ws", get(websocket::ws_handler))
        .merge(mcp)
        .nest("/api", api)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        // Batas body 55 MB: kirim media via JSON data URL (base64) bisa besar.
        .layer(DefaultBodyLimit::max(55 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
