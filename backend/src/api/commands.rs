//! Custom command per bot: CRUD + uji kirim.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::automation::render_template;
use crate::bot::get_bot_token;
use crate::bot::telegram::SendMessageParams;
use crate::error::{AppError, AppResult, Envelope};
use crate::models::{new_id, now_rfc3339, BotCommand, Paginated, Pagination};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateCommandRequest {
    pub bot_id: String,
    pub name: String,
    pub description: Option<String>,
    pub response_template: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub permissions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommandRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub response_template: Option<String>,
    pub aliases: Option<Vec<String>>,
    pub permissions: Option<Vec<String>>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct TestCommandRequest {
    #[serde(deserialize_with = "crate::models::de_chat_id")]
    pub chat_id: i64,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListCommandsQuery {
    pub bot_id: Option<String>,
    #[serde(flatten)]
    pub pagination: Pagination,
}

fn validate_command_name(name: &str) -> AppResult<String> {
    let normalized = name.trim().trim_start_matches('/').to_lowercase();
    if normalized.is_empty() || normalized.len() > 32 {
        return Err(AppError::Validation(
            "nama command harus 1–32 karakter".to_string(),
        ));
    }
    if !normalized
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AppError::Validation(
            "nama command hanya boleh huruf kecil, angka, dan underscore".to_string(),
        ));
    }
    Ok(normalized)
}

async fn fetch_command(state: &AppState, id: &str) -> AppResult<BotCommand> {
    sqlx::query_as::<_, BotCommand>("SELECT * FROM bot_commands WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("command {id} tidak ditemukan")))
}

pub async fn list_commands(
    State(state): State<AppState>,
    Query(q): Query<ListCommandsQuery>,
) -> AppResult<Json<Envelope<Paginated<BotCommand>>>> {
    let (page, per_page) = q.pagination.normalized();
    let offset = q.pagination.offset();
    let (items, total) = match &q.bot_id {
        Some(bot_id) => (
            sqlx::query_as::<_, BotCommand>(
                "SELECT * FROM bot_commands WHERE bot_id = ? ORDER BY name LIMIT ? OFFSET ?",
            )
            .bind(bot_id)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bot_commands WHERE bot_id = ?")
                .bind(bot_id)
                .fetch_one(&state.pool)
                .await?,
        ),
        None => (
            sqlx::query_as::<_, BotCommand>(
                "SELECT * FROM bot_commands ORDER BY name LIMIT ? OFFSET ?",
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM bot_commands")
                .fetch_one(&state.pool)
                .await?,
        ),
    };
    Ok(Envelope::ok(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}

pub async fn create_command(
    State(state): State<AppState>,
    Json(req): Json<CreateCommandRequest>,
) -> AppResult<Json<Envelope<BotCommand>>> {
    if req.bot_id.trim().is_empty() {
        return Err(AppError::Validation("bot_id tidak boleh kosong".to_string()));
    }
    let bot_exists: Option<(String,)> = sqlx::query_as("SELECT id FROM bots WHERE id = ?")
        .bind(&req.bot_id)
        .fetch_optional(&state.pool)
        .await?;
    if bot_exists.is_none() {
        return Err(AppError::NotFound(format!(
            "bot {} tidak ditemukan",
            req.bot_id
        )));
    }
    let name = validate_command_name(&req.name)?;
    let now = now_rfc3339();
    let id = new_id();
    let result = sqlx::query(
        "INSERT INTO bot_commands (id, bot_id, name, description, response_template, aliases, permissions, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.bot_id)
    .bind(&name)
    .bind(req.description.as_deref().unwrap_or(""))
    .bind(req.response_template.as_deref().unwrap_or(""))
    .bind(serde_json::to_string(&req.aliases.unwrap_or_default()).unwrap())
    .bind(serde_json::to_string(&req.permissions.unwrap_or_default()).unwrap())
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await;
    if let Err(e) = result {
        if e.to_string().contains("UNIQUE") {
            return Err(AppError::Conflict(format!(
                "command '/{name}' sudah ada untuk bot ini"
            )));
        }
        return Err(e.into());
    }
    Ok(Envelope::ok(fetch_command(&state, &id).await?))
}

pub async fn get_command(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<BotCommand>>> {
    Ok(Envelope::ok(fetch_command(&state, &id).await?))
}

pub async fn update_command(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateCommandRequest>,
) -> AppResult<Json<Envelope<BotCommand>>> {
    let mut cmd = fetch_command(&state, &id).await?;
    if let Some(name) = req.name {
        cmd.name = validate_command_name(&name)?;
    }
    if let Some(d) = req.description {
        cmd.description = d;
    }
    if let Some(t) = req.response_template {
        cmd.response_template = t;
    }
    if let Some(a) = req.aliases {
        cmd.aliases = serde_json::to_string(&a).unwrap();
    }
    if let Some(p) = req.permissions {
        cmd.permissions = serde_json::to_string(&p).unwrap();
    }
    if let Some(e) = req.is_enabled {
        cmd.is_enabled = if e { 1 } else { 0 };
    }
    cmd.updated_at = now_rfc3339();
    let result = sqlx::query(
        "UPDATE bot_commands SET name = ?, description = ?, response_template = ?, \
         aliases = ?, permissions = ?, is_enabled = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&cmd.name)
    .bind(&cmd.description)
    .bind(&cmd.response_template)
    .bind(&cmd.aliases)
    .bind(&cmd.permissions)
    .bind(cmd.is_enabled)
    .bind(&cmd.updated_at)
    .bind(&cmd.id)
    .execute(&state.pool)
    .await;
    if let Err(e) = result {
        if e.to_string().contains("UNIQUE") {
            return Err(AppError::Conflict(format!(
                "command '/{}' sudah ada untuk bot ini",
                cmd.name
            )));
        }
        return Err(e.into());
    }
    Ok(Envelope::ok(cmd))
}

pub async fn delete_command(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    fetch_command(&state, &id).await?;
    sqlx::query("DELETE FROM bot_commands WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({"deleted": true, "id": id})))
}

/// Uji command: kirim response_template ke chat_id yang dipilih.
pub async fn test_command(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<TestCommandRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    if req.chat_id == 0 {
        return Err(AppError::Validation("chat_id tidak boleh kosong".to_string()));
    }
    let cmd = fetch_command(&state, &id).await?;
    if cmd.response_template.trim().is_empty() {
        return Err(AppError::Validation(
            "command ini tidak punya response_template untuk diuji".to_string(),
        ));
    }
    let context = json!({
        "first_name": "Test",
        "username": "test_user",
        "chat_id": req.chat_id,
        "text": format!("/{}", cmd.name),
    });
    let text = render_template(&cmd.response_template, &context);
    let token = get_bot_token(&state, &cmd.bot_id).await?;
    let sent = state
        .telegram
        .send_message(&token, &SendMessageParams {
            chat_id: req.chat_id,
            text,
            ..Default::default()
        })
        .await?;
    let _ = sqlx::query(
        "UPDATE bot_commands SET usage_count = usage_count + 1, updated_at = ? WHERE id = ?",
    )
    .bind(now_rfc3339())
    .bind(&cmd.id)
    .execute(&state.pool)
    .await;
    Ok(Envelope::ok(json!({
        "sent": true,
        "telegram_message_id": sent.message_id,
        "command": cmd.name,
    })))
}
