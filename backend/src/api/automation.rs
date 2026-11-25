//! Automation: CRUD workflow, history eksekusi, toggle.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::error::{AppError, AppResult, Envelope};
use crate::models::{
    new_id, now_rfc3339, AutomationExecution, AutomationWorkflow, Paginated, Pagination,
};
use crate::state::AppState;

pub const TRIGGER_TYPES: &[&str] = &[
    "new_user",
    "keyword",
    "command",
    "message_received",
    "scheduled",
    "user_joined_group",
];

#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    pub bot_id: String,
    pub name: String,
    pub description: Option<String>,
    pub trigger_type: String,
    pub trigger_config: Option<serde_json::Value>,
    pub conditions: Option<serde_json::Value>,
    pub actions: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub trigger_type: Option<String>,
    pub trigger_config: Option<serde_json::Value>,
    pub conditions: Option<serde_json::Value>,
    pub actions: Option<serde_json::Value>,
    pub is_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListWorkflowsQuery {
    pub bot_id: Option<String>,
    #[serde(flatten)]
    pub pagination: Pagination,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListExecutionsQuery {
    pub workflow_id: Option<String>,
    pub bot_id: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    pub pagination: Pagination,
}

fn validate_trigger(trigger: &str) -> AppResult<()> {
    if !TRIGGER_TYPES.contains(&trigger) {
        return Err(AppError::Validation(format!(
            "trigger_type tidak dikenal: '{trigger}' (yang didukung: {})",
            TRIGGER_TYPES.join(", ")
        )));
    }
    Ok(())
}

async fn fetch_workflow(state: &AppState, id: &str) -> AppResult<AutomationWorkflow> {
    sqlx::query_as::<_, AutomationWorkflow>("SELECT * FROM automation_workflows WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("workflow {id} tidak ditemukan")))
}

pub async fn list_workflows(
    State(state): State<AppState>,
    Query(q): Query<ListWorkflowsQuery>,
) -> AppResult<Json<Envelope<Paginated<AutomationWorkflow>>>> {
    let (page, per_page) = q.pagination.normalized();
    let offset = q.pagination.offset();
    let (items, total) = match &q.bot_id {
        Some(bot_id) => (
            sqlx::query_as::<_, AutomationWorkflow>(
                "SELECT * FROM automation_workflows WHERE bot_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(bot_id)
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM automation_workflows WHERE bot_id = ?",
            )
            .bind(bot_id)
            .fetch_one(&state.pool)
            .await?,
        ),
        None => (
            sqlx::query_as::<_, AutomationWorkflow>(
                "SELECT * FROM automation_workflows ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(per_page)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?,
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM automation_workflows")
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

pub async fn create_workflow(
    State(state): State<AppState>,
    Json(req): Json<CreateWorkflowRequest>,
) -> AppResult<Json<Envelope<AutomationWorkflow>>> {
    if req.bot_id.trim().is_empty() {
        return Err(AppError::Validation("bot_id tidak boleh kosong".to_string()));
    }
    if req.name.trim().is_empty() {
        return Err(AppError::Validation("nama workflow tidak boleh kosong".to_string()));
    }
    validate_trigger(&req.trigger_type)?;
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
    let actions = req.actions.clone().unwrap_or(json!([]));
    if !actions.is_array() {
        return Err(AppError::Validation("actions harus berupa array JSON".to_string()));
    }
    let now = now_rfc3339();
    let id = new_id();
    sqlx::query(
        "INSERT INTO automation_workflows (id, bot_id, name, description, trigger_type, trigger_config, conditions, actions, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.bot_id)
    .bind(req.name.trim())
    .bind(req.description.as_deref())
    .bind(&req.trigger_type)
    .bind(req.trigger_config.unwrap_or(json!({})).to_string())
    .bind(req.conditions.unwrap_or(json!([])).to_string())
    .bind(actions.to_string())
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(fetch_workflow(&state, &id).await?))
}

pub async fn get_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<AutomationWorkflow>>> {
    Ok(Envelope::ok(fetch_workflow(&state, &id).await?))
}

pub async fn update_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> AppResult<Json<Envelope<AutomationWorkflow>>> {
    let mut wf = fetch_workflow(&state, &id).await?;
    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err(AppError::Validation(
                "nama workflow tidak boleh kosong".to_string(),
            ));
        }
        wf.name = name.trim().to_string();
    }
    if let Some(d) = req.description {
        wf.description = Some(d);
    }
    if let Some(t) = req.trigger_type {
        validate_trigger(&t)?;
        wf.trigger_type = t;
    }
    if let Some(c) = req.trigger_config {
        wf.trigger_config = c.to_string();
    }
    if let Some(c) = req.conditions {
        wf.conditions = c.to_string();
    }
    if let Some(a) = req.actions {
        if !a.is_array() {
            return Err(AppError::Validation(
                "actions harus berupa array JSON".to_string(),
            ));
        }
        wf.actions = a.to_string();
    }
    if let Some(e) = req.is_enabled {
        wf.is_enabled = if e { 1 } else { 0 };
    }
    wf.updated_at = now_rfc3339();
    sqlx::query(
        "UPDATE automation_workflows SET name = ?, description = ?, trigger_type = ?, \
         trigger_config = ?, conditions = ?, actions = ?, is_enabled = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&wf.name)
    .bind(&wf.description)
    .bind(&wf.trigger_type)
    .bind(&wf.trigger_config)
    .bind(&wf.conditions)
    .bind(&wf.actions)
    .bind(wf.is_enabled)
    .bind(&wf.updated_at)
    .bind(&wf.id)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(wf))
}

pub async fn delete_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    fetch_workflow(&state, &id).await?;
    sqlx::query("DELETE FROM automation_workflows WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({"deleted": true, "id": id})))
}

pub async fn toggle_workflow(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<AutomationWorkflow>>> {
    let wf = fetch_workflow(&state, &id).await?;
    let new_value = if wf.is_enabled == 1 { 0 } else { 1 };
    sqlx::query("UPDATE automation_workflows SET is_enabled = ?, updated_at = ? WHERE id = ?")
        .bind(new_value)
        .bind(now_rfc3339())
        .bind(&id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(fetch_workflow(&state, &id).await?))
}

/// Riwayat eksekusi untuk SATU workflow: GET /automation/:id/executions.
pub async fn list_workflow_executions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(mut q): Query<ListExecutionsQuery>,
) -> AppResult<Json<Envelope<Paginated<AutomationExecution>>>> {
    // Pastikan workflow ada (404 bila tidak).
    fetch_workflow(&state, &id).await?;
    q.workflow_id = Some(id);
    list_executions(State(state), Query(q)).await
}

pub async fn list_executions(
    State(state): State<AppState>,
    Query(q): Query<ListExecutionsQuery>,
) -> AppResult<Json<Envelope<Paginated<AutomationExecution>>>> {
    let (page, per_page) = q.pagination.normalized();
    let offset = q.pagination.offset();
    let mut qb =
        sqlx::QueryBuilder::new("SELECT * FROM automation_executions WHERE 1=1");
    if let Some(w) = &q.workflow_id {
        qb.push(" AND workflow_id = ").push_bind(w);
    }
    if let Some(b) = &q.bot_id {
        qb.push(" AND bot_id = ").push_bind(b);
    }
    if let Some(s) = &q.status {
        qb.push(" AND status = ").push_bind(s);
    }
    qb.push(" ORDER BY created_at DESC LIMIT ")
        .push_bind(per_page)
        .push(" OFFSET ")
        .push_bind(offset);
    let items = qb
        .build_query_as::<AutomationExecution>()
        .fetch_all(&state.pool)
        .await?;

    let mut count_qb =
        sqlx::QueryBuilder::new("SELECT COUNT(*) FROM automation_executions WHERE 1=1");
    if let Some(w) = &q.workflow_id {
        count_qb.push(" AND workflow_id = ").push_bind(w);
    }
    if let Some(b) = &q.bot_id {
        count_qb.push(" AND bot_id = ").push_bind(b);
    }
    if let Some(s) = &q.status {
        count_qb.push(" AND status = ").push_bind(s);
    }
    let total = count_qb
        .build_query_scalar::<i64>()
        .fetch_one(&state.pool)
        .await?;

    Ok(Envelope::ok(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}
