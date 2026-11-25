//! MCP server (v1.1): JSON-RPC 2.0 Streamable HTTP di `POST /mcp` (root,
//! BUKAN `/api/mcp`). Auth: middleware yang sama dengan API — header
//! `X-API-Key` berisi API key env ATAU personal token (`tc_...`).
//! Auth gagal dijawab 401 oleh middleware (bukan error JSON-RPC -32001);
//! pilihan ini konsisten dan didokumentasikan di `docs/MCP.md`.
//!
//! Method: `initialize`, `ping`, `tools/list`, `tools/call`.
//! Error JSON-RPC: tool tak dikenal / params tidak valid -> -32602,
//! method tak dikenal -> -32601, body bukan JSON -> -32700,
//! request bukan JSON-RPC 2.0 -> -32600.

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

use crate::api::analytics::compute_summary;
use crate::bot::get_bot_token;
use crate::bot::telegram::SendMessageParams;
use crate::error::AppError;
use crate::models::{new_id, now_rfc3339, Bot, BotUser, Message};
use crate::services::{lookup_template_vars, render_template_vars};
use crate::state::AppState;

pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
pub const MCP_SERVER_NAME: &str = "telecore-mcp";

#[derive(Debug)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl RpcError {
    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }
    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method tidak dikenal: '{method}'"),
        }
    }
}

fn tool_definitions() -> Value {
    json!([
        {"name": "list_bots", "description": "Daftar bot yang terdaftar di TeleCore.", "inputSchema": {"type": "object", "properties": {"status": {"type": "string", "description": "Filter status koneksi (connected/disconnected/error)."}}, "additionalProperties": false}},
        {"name": "get_bot", "description": "Detail satu bot berdasarkan id.", "inputSchema": {"type": "object", "properties": {"bot_id": {"type": "string"}}, "required": ["bot_id"], "additionalProperties": false}},
        {"name": "send_message", "description": "Kirim pesan teks via bot ke chat_id numerik. Mendukung template variables {{first_name}}, {{username}}, {{chat_title}}.", "inputSchema": {"type": "object", "properties": {"bot_id": {"type": "string"}, "chat_id": {"type": ["integer", "string"], "description": "ID chat numerik Telegram."}, "text": {"type": "string"}, "parse_mode": {"type": "string", "enum": ["Markdown", "MarkdownV2", "HTML"]}}, "required": ["bot_id", "chat_id", "text"], "additionalProperties": false}},
        {"name": "list_users", "description": "Daftar pengguna bot (bot_users).", "inputSchema": {"type": "object", "properties": {"bot_id": {"type": "string"}, "q": {"type": "string", "description": "Cari username/nama."}, "limit": {"type": "integer", "default": 20}}, "additionalProperties": false}},
        {"name": "get_analytics_summary", "description": "Ringkasan analytics yang sama dengan GET /api/analytics/summary.", "inputSchema": {"type": "object", "properties": {"bot_id": {"type": "string"}, "from": {"type": "string", "description": "RFC3339."}, "to": {"type": "string", "description": "RFC3339."}}, "additionalProperties": false}},
        {"name": "list_workflows", "description": "Daftar automation workflow.", "inputSchema": {"type": "object", "properties": {"bot_id": {"type": "string"}}, "additionalProperties": false}},
        {"name": "list_scheduled_tasks", "description": "Daftar scheduled task.", "inputSchema": {"type": "object", "properties": {"bot_id": {"type": "string"}}, "additionalProperties": false}}
    ])
}

/// Dispatch method yang TIDAK butuh DB/state (pure — diuji unit).
/// `tools/call` ditangani terpisah di [`execute_tool`] karena butuh state.
pub fn dispatch_stateless(method: &str, _params: &Value) -> Result<Value, RpcError> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": MCP_SERVER_NAME, "version": env!("CARGO_PKG_VERSION")},
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tool_definitions()})),
        other => Err(RpcError::method_not_found(other)),
    }
}

fn parse_chat_id(v: &Value) -> Result<i64, RpcError> {
    match v {
        Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| RpcError::invalid_params("chat_id terlalu besar")),
        Value::String(s) => s
            .trim()
            .parse::<i64>()
            .map_err(|_| RpcError::invalid_params("chat_id harus berupa angka (ID numerik Telegram)")),
        _ => Err(RpcError::invalid_params("chat_id harus berupa angka")),
    }
}

fn required_string(args: &Value, field: &str) -> Result<String, RpcError> {
    args.get(field)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RpcError::invalid_params(format!("{field} wajib diisi")))
}

/// Eksekusi satu tool MCP. Tool tak dikenal -> -32602.
pub async fn execute_tool(
    state: &AppState,
    name: &str,
    args: &Value,
) -> Result<Value, RpcError> {
    let args = if args.is_null() { &json!({}) } else { args };
    let bot_id = args.get("bot_id").and_then(|v| v.as_str());
    match name {
        "list_bots" => {
            let status = args.get("status").and_then(|v| v.as_str());
            let (bots, total) = match status {
                Some(s) => (
                    sqlx::query_as::<_, Bot>(
                        "SELECT * FROM bots WHERE status = ? ORDER BY created_at DESC LIMIT 50",
                    )
                    .bind(s)
                    .fetch_all(&state.pool)
                    .await
                    .map_err(|e| RpcError::invalid_params(format!("gagal memuat bot: {e}")))?,
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bots WHERE status = ?")
                        .bind(s)
                        .fetch_one(&state.pool)
                        .await
                        .unwrap_or(0),
                ),
                None => (
                    sqlx::query_as::<_, Bot>(
                        "SELECT * FROM bots ORDER BY created_at DESC LIMIT 50",
                    )
                    .fetch_all(&state.pool)
                    .await
                    .map_err(|e| RpcError::invalid_params(format!("gagal memuat bot: {e}")))?,
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bots")
                        .fetch_one(&state.pool)
                        .await
                        .unwrap_or(0),
                ),
            };
            Ok(json!({"bots": bots, "total": total}))
        }
        "get_bot" => {
            let id = required_string(args, "bot_id")?;
            let bot = sqlx::query_as::<_, Bot>("SELECT * FROM bots WHERE id = ?")
                .bind(&id)
                .fetch_optional(&state.pool)
                .await
                .map_err(|e| RpcError::invalid_params(format!("gagal memuat bot: {e}")))?
                .ok_or_else(|| RpcError::invalid_params(format!("bot {id} tidak ditemukan")))?;
            Ok(json!(bot))
        }
        "send_message" => {
            let bot_id = required_string(args, "bot_id")?;
            let chat_id = parse_chat_id(
                args.get("chat_id")
                    .ok_or_else(|| RpcError::invalid_params("chat_id wajib diisi"))?,
            )?;
            let text = required_string(args, "text")?;
            let parse_mode = args
                .get("parse_mode")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(mode) = &parse_mode {
                if !matches!(mode.as_str(), "Markdown" | "MarkdownV2" | "HTML") {
                    return Err(RpcError::invalid_params(
                        "parse_mode harus Markdown, MarkdownV2, atau HTML",
                    ));
                }
            }
            let vars = lookup_template_vars(state, &bot_id, chat_id).await;
            let rendered = render_template_vars(&text, &vars);
            let token = get_bot_token(state, &bot_id)
                .await
                .map_err(|e| RpcError::invalid_params(e.to_string()))?;
            let sent = state
                .telegram
                .send_message(
                    &token,
                    &SendMessageParams {
                        chat_id,
                        text: rendered.clone(),
                        parse_mode: parse_mode.clone(),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|e: AppError| RpcError::invalid_params(e.to_string()))?;
            let id = new_id();
            let now = now_rfc3339();
            sqlx::query(
                "INSERT INTO messages (id, bot_id, chat_id, telegram_message_id, direction, \
                 message_type, content, status, parse_mode, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, 'outbound', 'text', ?, 'sent', ?, ?, ?)",
            )
            .bind(&id)
            .bind(&bot_id)
            .bind(chat_id)
            .bind(sent.message_id)
            .bind(&rendered)
            .bind(parse_mode.as_deref())
            .bind(&now)
            .bind(&now)
            .execute(&state.pool)
            .await
            .map_err(|e| RpcError::invalid_params(format!("gagal menyimpan pesan: {e}")))?;
            let msg = sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE id = ?")
                .bind(&id)
                .fetch_one(&state.pool)
                .await
                .map_err(|e| RpcError::invalid_params(format!("gagal memuat pesan: {e}")))?;
            state.hub.publish(
                "message",
                json!({"bot_id": bot_id, "chat_id": chat_id, "direction": "outbound", "message_type": "text"}),
            );
            Ok(json!(msg))
        }
        "list_users" => {
            let limit = args
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(20)
                .clamp(1, 100);
            let q = args.get("q").and_then(|v| v.as_str());
            let mut qb =
                sqlx::QueryBuilder::new("SELECT * FROM bot_users WHERE 1=1");
            if let Some(b) = bot_id {
                qb.push(" AND bot_id = ").push_bind(b);
            }
            if let Some(search) = q {
                let pattern = format!("%{}%", search.trim());
                qb.push(" AND (username LIKE ")
                    .push_bind(pattern.clone())
                    .push(" OR first_name LIKE ")
                    .push_bind(pattern.clone())
                    .push(" OR last_name LIKE ")
                    .push_bind(pattern)
                    .push(")");
            }
            qb.push(" ORDER BY last_seen_at DESC LIMIT ").push_bind(limit);
            let users = qb
                .build_query_as::<BotUser>()
                .fetch_all(&state.pool)
                .await
                .map_err(|e| RpcError::invalid_params(format!("gagal memuat user: {e}")))?;
            Ok(json!({"users": users, "total": users.len()}))
        }
        "get_analytics_summary" => {
            let from = args.get("from").and_then(|v| v.as_str()).map(|s| s.to_string());
            let to = args.get("to").and_then(|v| v.as_str()).map(|s| s.to_string());
            let bot_id = bot_id.map(|s| s.to_string());
            compute_summary(state, &bot_id, &from, &to)
                .await
                .map_err(|e| RpcError::invalid_params(e.to_string()))
        }
        "list_workflows" => {
            let (items, total) = match bot_id {
                Some(b) => (
                    sqlx::query_as::<_, crate::models::AutomationWorkflow>(
                        "SELECT * FROM automation_workflows WHERE bot_id = ? ORDER BY created_at DESC LIMIT 50",
                    )
                    .bind(b)
                    .fetch_all(&state.pool)
                    .await
                    .map_err(|e| RpcError::invalid_params(format!("gagal memuat workflow: {e}")))?,
                    sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM automation_workflows WHERE bot_id = ?",
                    )
                    .bind(b)
                    .fetch_one(&state.pool)
                    .await
                    .unwrap_or(0),
                ),
                None => (
                    sqlx::query_as::<_, crate::models::AutomationWorkflow>(
                        "SELECT * FROM automation_workflows ORDER BY created_at DESC LIMIT 50",
                    )
                    .fetch_all(&state.pool)
                    .await
                    .map_err(|e| RpcError::invalid_params(format!("gagal memuat workflow: {e}")))?,
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM automation_workflows")
                        .fetch_one(&state.pool)
                        .await
                        .unwrap_or(0),
                ),
            };
            Ok(json!({"workflows": items, "total": total}))
        }
        "list_scheduled_tasks" => {
            let (items, total) = match bot_id {
                Some(b) => (
                    sqlx::query_as::<_, crate::models::ScheduledTask>(
                        "SELECT * FROM scheduled_tasks WHERE bot_id = ? ORDER BY next_run_at LIMIT 50",
                    )
                    .bind(b)
                    .fetch_all(&state.pool)
                    .await
                    .map_err(|e| RpcError::invalid_params(format!("gagal memuat task: {e}")))?,
                    sqlx::query_scalar::<_, i64>(
                        "SELECT COUNT(*) FROM scheduled_tasks WHERE bot_id = ?",
                    )
                    .bind(b)
                    .fetch_one(&state.pool)
                    .await
                    .unwrap_or(0),
                ),
                None => (
                    sqlx::query_as::<_, crate::models::ScheduledTask>(
                        "SELECT * FROM scheduled_tasks ORDER BY next_run_at LIMIT 50",
                    )
                    .fetch_all(&state.pool)
                    .await
                    .map_err(|e| RpcError::invalid_params(format!("gagal memuat task: {e}")))?,
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM scheduled_tasks")
                        .fetch_one(&state.pool)
                        .await
                        .unwrap_or(0),
                ),
            };
            Ok(json!({"tasks": items, "total": total}))
        }
        other => Err(RpcError::invalid_params(format!(
            "tool tidak dikenal: '{other}'"
        ))),
    }
}

fn rpc_result(id: &Value, result: Value) -> Json<Value> {
    Json(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Json<Value> {
    Json(json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}))
}

/// Handler `POST /mcp` — satu request JSON-RPC 2.0 per HTTP request
/// (Streamable HTTP; batch tidak didukung).
pub async fn mcp_handler(State(state): State<AppState>, body: Bytes) -> Response {
    let req: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::OK,
                rpc_error(&Value::Null, -32700, "Parse error: body bukan JSON valid"),
            )
                .into_response();
        }
    };
    if req.is_array() {
        return (
            StatusCode::OK,
            rpc_error(
                &Value::Null,
                -32600,
                "Invalid Request: batch request tidak didukung",
            ),
        )
            .into_response();
    }
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    // Notifikasi (tanpa id) tidak dijawab — 202 kosong.
    if req.get("id").is_none() {
        return StatusCode::ACCEPTED.into_response();
    }
    if req.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
        return (
            StatusCode::OK,
            rpc_error(&id, -32600, "Invalid Request: jsonrpc harus \"2.0\""),
        )
            .into_response();
    }
    let method = match req.get("method").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => {
            return (
                StatusCode::OK,
                rpc_error(&id, -32600, "Invalid Request: method wajib string"),
            )
                .into_response();
        }
    };
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    if method == "tools/call" {
        let name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return (
                    StatusCode::OK,
                    rpc_error(&id, -32602, "tools/call membutuhkan params.name"),
                )
                    .into_response();
            }
        };
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        match execute_tool(&state, &name, &args).await {
            Ok(result) => (
                StatusCode::OK,
                rpc_result(&id, json!({"content": [{"type": "text", "text": result.to_string()}], "structuredContent": result})),
            )
                .into_response(),
            Err(e) => (StatusCode::OK, rpc_error(&id, e.code, &e.message)).into_response(),
        }
    } else {
        match dispatch_stateless(&method, &params) {
            Ok(result) => (StatusCode::OK, rpc_result(&id, result)).into_response(),
            Err(e) => (StatusCode::OK, rpc_error(&id, e.code, &e.message)).into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_initialize_returns_server_info() {
        let result = dispatch_stateless("initialize", &json!({})).unwrap();
        assert_eq!(result["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], "telecore-mcp");
        assert_eq!(result["serverInfo"]["version"], "1.3.0");
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[test]
    fn dispatch_ping_returns_empty_object() {
        assert_eq!(dispatch_stateless("ping", &json!({})).unwrap(), json!({}));
    }

    #[test]
    fn dispatch_tools_list_contains_all_seven_tools() {
        let result = dispatch_stateless("tools/list", &json!({})).unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        for expected in [
            "list_bots",
            "get_bot",
            "send_message",
            "list_users",
            "get_analytics_summary",
            "list_workflows",
            "list_scheduled_tasks",
        ] {
            assert!(names.contains(&expected), "tool {expected} hilang dari tools/list");
        }
    }

    #[test]
    fn dispatch_unknown_method_returns_minus_32601() {
        let err = dispatch_stateless("resources/list", &json!({})).unwrap_err();
        assert_eq!(err.code, -32601);
    }

    #[tokio::test]
    async fn execute_unknown_tool_returns_minus_32602() {
        // Tanpa DB: tool tak dikenal ditolak sebelum menyentuh database.
        // State dibangun di atas pool in-memory (migrasi dijalankan).
        let state = test_state().await;
        let err = execute_tool(&state, "tool_tidak_ada", &json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn execute_list_bots_valid_call() {
        let state = test_state().await;
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO bots (id, name, token_encrypted, token_masked, status, created_at, updated_at) \
             VALUES ('bot-1', 'BotTest', 'enc', '****1234', 'connected', ?, ?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await
        .unwrap();
        let result = execute_tool(&state, "list_bots", &json!({})).await.unwrap();
        assert_eq!(result["total"], 1);
        assert_eq!(result["bots"][0]["name"], "BotTest");
        // Token terenkripsi tidak pernah keluar via MCP.
        assert!(result["bots"][0].get("token_encrypted").is_none());

        let result = execute_tool(&state, "get_bot", &json!({"bot_id": "bot-1"}))
            .await
            .unwrap();
        assert_eq!(result["name"], "BotTest");
        // get_bot dengan id yang tidak ada -> -32602.
        let err = execute_tool(&state, "get_bot", &json!({"bot_id": "tidak-ada"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
    }

    /// AppState untuk test: SQLite in-memory + migrasi v1.1.
    async fn test_state() -> AppState {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(":memory:")
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let config = crate::config::Config {
            host: "127.0.0.1".to_string(),
            port: 8080,
            database_url: "sqlite::memory:".to_string(),
            api_key: "test-key".to_string(),
            rust_log: "info".to_string(),
        };
        let crypto = std::sync::Arc::new(crate::security::Crypto::from_key_bytes([9u8; 32]));
        let telegram = crate::bot::telegram::TelegramClient::new().unwrap();
        AppState::new(pool, config, crypto, telegram)
    }
}
