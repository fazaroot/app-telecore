//! CRM v1.3: tag pengguna, catatan pengguna, segmen, dan timeline.
//!
//! Kontrak API (dipakai frontend):
//! - Tags: GET/POST /api/tags, PUT/DELETE /api/tags/{id};
//!   GET/POST /api/users/{id}/tags, DELETE /api/users/{id}/tags/{tag_id}.
//! - Notes: GET/POST /api/users/{id}/notes, DELETE /api/users/{id}/notes/{note_id}.
//!   Field `notes` tunggal di BotUser TIDAK diubah (kompatibilitas).
//! - Segmen: GET/POST /api/segments, GET/PUT/DELETE /api/segments/{id},
//!   POST /api/segments/preview, GET /api/segments/{id}/members,
//!   POST /api/segments/{id}/follow-up.
//! - Timeline: GET /api/users/{id}/timeline.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::api::messages::{create_broadcast, BroadcastRecipient, BroadcastRequest};
use crate::error::{AppError, AppResult, Envelope};
use crate::models::{
    new_id, now_rfc3339, BotUser, Message, Order, Paginated, Pagination, Segment, UserNote,
    UserTag,
};
use crate::services::segments::{
    is_valid_hex_color, parse_criteria, segment_member_count, segment_members,
    SegmentCriteria,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn fetch_user(state: &AppState, id: &str) -> AppResult<BotUser> {
    sqlx::query_as::<_, BotUser>("SELECT * FROM bot_users WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("user {id} tidak ditemukan")))
}

async fn fetch_tag(state: &AppState, id: &str) -> AppResult<UserTag> {
    sqlx::query_as::<_, UserTag>("SELECT * FROM user_tags WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("tag {id} tidak ditemukan")))
}

async fn fetch_segment(state: &AppState, id: &str) -> AppResult<Segment> {
    sqlx::query_as::<_, Segment>("SELECT * FROM segments WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("segmen {id} tidak ditemukan")))
}

fn criteria_to_json(c: &SegmentCriteria) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    if let Some(t) = &c.any_tags {
        m.insert("any_tags".to_string(), json!(t));
    }
    if let Some(v) = c.has_ordered {
        m.insert("has_ordered".to_string(), json!(v));
    }
    if let Some(v) = c.last_active_within_days {
        m.insert("last_active_within_days".to_string(), json!(v));
    }
    if let Some(v) = c.last_active_over_days_ago {
        m.insert("last_active_over_days_ago".to_string(), json!(v));
    }
    if let Some(v) = c.min_messages {
        m.insert("min_messages".to_string(), json!(v));
    }
    serde_json::Value::Object(m)
}

fn validate_tag_input(name: &str, color: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::Validation("name tag tidak boleh kosong".to_string()));
    }
    if !is_valid_hex_color(color) {
        return Err(AppError::Validation(
            "color harus format hex #RRGGBB (contoh: #526FE5)".to_string(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
    #[serde(default = "default_tag_color")]
    pub color: String,
}

fn default_tag_color() -> String {
    "#526FE5".to_string()
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateTagRequest {
    pub name: Option<String>,
    pub color: Option<String>,
}

pub async fn list_tags(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<Vec<UserTag>>>> {
    let tags = sqlx::query_as::<_, UserTag>(
        "SELECT * FROM user_tags ORDER BY created_at ASC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Envelope::ok(tags))
}

pub async fn create_tag(
    State(state): State<AppState>,
    Json(req): Json<CreateTagRequest>,
) -> AppResult<Json<Envelope<UserTag>>> {
    validate_tag_input(&req.name, &req.color)?;
    let tag = UserTag {
        id: new_id(),
        name: req.name.trim().to_string(),
        color: req.color.trim().to_string(),
        created_at: now_rfc3339(),
    };
    sqlx::query(
        "INSERT INTO user_tags (id, name, color, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&tag.id)
    .bind(&tag.name)
    .bind(&tag.color)
    .bind(&tag.created_at)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(tag))
}

pub async fn update_tag(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTagRequest>,
) -> AppResult<Json<Envelope<UserTag>>> {
    let mut tag = fetch_tag(&state, &id).await?;
    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err(AppError::Validation(
                "name tag tidak boleh kosong".to_string(),
            ));
        }
        tag.name = name.trim().to_string();
    }
    if let Some(color) = req.color {
        if !is_valid_hex_color(color.trim()) {
            return Err(AppError::Validation(
                "color harus format hex #RRGGBB (contoh: #526FE5)".to_string(),
            ));
        }
        tag.color = color.trim().to_string();
    }
    sqlx::query("UPDATE user_tags SET name = ?, color = ? WHERE id = ?")
        .bind(&tag.name)
        .bind(&tag.color)
        .bind(&tag.id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(tag))
}

pub async fn delete_tag(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let tag = fetch_tag(&state, &id).await?;
    // Hapus assignment-nya juga (cascade manual — jangan biarkan tag yang
    // masih dipakai menggantung tanpa tag).
    sqlx::query("DELETE FROM user_tag_assignments WHERE tag_id = ?")
        .bind(&tag.id)
        .execute(&state.pool)
        .await?;
    sqlx::query("DELETE FROM user_tags WHERE id = ?")
        .bind(&tag.id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({"success": true, "deleted_id": tag.id})))
}

pub async fn list_user_tags(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<Vec<UserTag>>>> {
    let user = fetch_user(&state, &id).await?;
    let tags = sqlx::query_as::<_, UserTag>(
        "SELECT t.* FROM user_tags t \
         JOIN user_tag_assignments a ON a.tag_id = t.id \
         WHERE a.user_id = ? ORDER BY t.created_at ASC",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Envelope::ok(tags))
}

#[derive(Debug, Deserialize)]
pub struct AssignTagRequest {
    pub tag_id: String,
}

pub async fn assign_user_tag(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AssignTagRequest>,
) -> AppResult<Json<Envelope<UserTag>>> {
    let user = fetch_user(&state, &id).await?;
    let tag = fetch_tag(&state, &req.tag_id).await?;
    // Idempotent: assign ulang tag yang sama bukan error.
    sqlx::query(
        "INSERT OR IGNORE INTO user_tag_assignments (user_id, tag_id, assigned_at) \
         VALUES (?, ?, ?)",
    )
    .bind(&user.id)
    .bind(&tag.id)
    .bind(now_rfc3339())
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(tag))
}

pub async fn remove_user_tag(
    State(state): State<AppState>,
    Path((id, tag_id)): Path<(String, String)>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let user = fetch_user(&state, &id).await?;
    let _ = fetch_tag(&state, &tag_id).await?;
    let res = sqlx::query(
        "DELETE FROM user_tag_assignments WHERE user_id = ? AND tag_id = ?",
    )
    .bind(&user.id)
    .bind(&tag_id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "tag tersebut tidak terpasang pada user ini".to_string(),
        ));
    }
    Ok(Envelope::ok(json!({"success": true})))
}

// ---------------------------------------------------------------------------
// Notes (multi-catatan; field `notes` tunggal di BotUser dibiarkan)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateNoteRequest {
    pub note: String,
}

pub async fn list_user_notes(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<Vec<UserNote>>>> {
    let user = fetch_user(&state, &id).await?;
    let notes = sqlx::query_as::<_, UserNote>(
        "SELECT * FROM user_notes WHERE user_id = ? ORDER BY created_at DESC LIMIT 100",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Envelope::ok(notes))
}

pub async fn create_user_note(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateNoteRequest>,
) -> AppResult<Json<Envelope<UserNote>>> {
    let user = fetch_user(&state, &id).await?;
    if req.note.trim().is_empty() {
        return Err(AppError::Validation("note tidak boleh kosong".to_string()));
    }
    let note = UserNote {
        id: new_id(),
        user_id: user.id,
        note: req.note.trim().to_string(),
        created_at: now_rfc3339(),
    };
    sqlx::query(
        "INSERT INTO user_notes (id, user_id, note, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(&note.id)
    .bind(&note.user_id)
    .bind(&note.note)
    .bind(&note.created_at)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(note))
}

pub async fn delete_user_note(
    State(state): State<AppState>,
    Path((id, note_id)): Path<(String, String)>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let user = fetch_user(&state, &id).await?;
    let res = sqlx::query("DELETE FROM user_notes WHERE id = ? AND user_id = ?")
        .bind(&note_id)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!(
            "catatan {note_id} tidak ditemukan untuk user ini"
        )));
    }
    Ok(Envelope::ok(json!({"success": true})))
}

// ---------------------------------------------------------------------------
// Segmen
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateSegmentRequest {
    pub name: String,
    pub criteria: serde_json::Value,
    pub bot_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateSegmentRequest {
    pub name: Option<String>,
    pub criteria: Option<serde_json::Value>,
    pub bot_id: Option<String>,
}

pub async fn list_segments(
    State(state): State<AppState>,
) -> AppResult<Json<Envelope<Vec<Segment>>>> {
    let items = sqlx::query_as::<_, Segment>(
        "SELECT * FROM segments ORDER BY created_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Envelope::ok(items))
}

pub async fn create_segment(
    State(state): State<AppState>,
    Json(req): Json<CreateSegmentRequest>,
) -> AppResult<Json<Envelope<Segment>>> {
    if req.name.trim().is_empty() {
        return Err(AppError::Validation("name segmen tidak boleh kosong".to_string()));
    }
    let criteria = parse_criteria(&req.criteria)?;
    let now = now_rfc3339();
    let seg = Segment {
        id: new_id(),
        name: req.name.trim().to_string(),
        bot_id: req.bot_id.filter(|s| !s.trim().is_empty()),
        criteria: criteria_to_json(&criteria).to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    sqlx::query(
        "INSERT INTO segments (id, name, bot_id, criteria, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&seg.id)
    .bind(&seg.name)
    .bind(&seg.bot_id)
    .bind(&seg.criteria)
    .bind(&seg.created_at)
    .bind(&seg.updated_at)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(seg))
}

pub async fn get_segment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<Segment>>> {
    Ok(Envelope::ok(fetch_segment(&state, &id).await?))
}

pub async fn update_segment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSegmentRequest>,
) -> AppResult<Json<Envelope<Segment>>> {
    let mut seg = fetch_segment(&state, &id).await?;
    if let Some(name) = req.name {
        if name.trim().is_empty() {
            return Err(AppError::Validation(
                "name segmen tidak boleh kosong".to_string(),
            ));
        }
        seg.name = name.trim().to_string();
    }
    if let Some(criteria) = req.criteria {
        let parsed = parse_criteria(&criteria)?;
        seg.criteria = criteria_to_json(&parsed).to_string();
    }
    if let Some(bot_id) = req.bot_id {
        seg.bot_id = if bot_id.trim().is_empty() {
            None
        } else {
            Some(bot_id)
        };
    }
    seg.updated_at = now_rfc3339();
    sqlx::query(
        "UPDATE segments SET name = ?, bot_id = ?, criteria = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&seg.name)
    .bind(&seg.bot_id)
    .bind(&seg.criteria)
    .bind(&seg.updated_at)
    .bind(&seg.id)
    .execute(&state.pool)
    .await?;
    Ok(Envelope::ok(seg))
}

pub async fn delete_segment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let seg = fetch_segment(&state, &id).await?;
    sqlx::query("DELETE FROM segments WHERE id = ?")
        .bind(&seg.id)
        .execute(&state.pool)
        .await?;
    Ok(Envelope::ok(json!({"success": true, "deleted_id": seg.id})))
}

#[derive(Debug, Deserialize)]
pub struct PreviewSegmentRequest {
    pub criteria: serde_json::Value,
    pub bot_id: Option<String>,
}

pub async fn preview_segment(
    State(state): State<AppState>,
    Json(req): Json<PreviewSegmentRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let criteria = parse_criteria(&req.criteria)?;
    let count = segment_member_count(
        &state.pool,
        &criteria,
        req.bot_id.as_deref().filter(|s| !s.trim().is_empty()),
    )
    .await?;
    Ok(Envelope::ok(json!({"count": count})))
}

pub async fn segment_members_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> AppResult<Json<Envelope<Paginated<BotUser>>>> {
    let seg = fetch_segment(&state, &id).await?;
    let criteria = parse_criteria(&serde_json::from_str::<serde_json::Value>(&seg.criteria).unwrap_or(json!({})))?;
    let (page, per_page) = pagination.normalized();
    let offset = pagination.offset();
    let total = segment_member_count(&state.pool, &criteria, seg.bot_id.as_deref()).await?;
    let items = segment_members(
        &state.pool,
        &criteria,
        seg.bot_id.as_deref(),
        per_page,
        offset,
    )
    .await?;
    Ok(Envelope::ok(Paginated {
        items,
        total,
        page,
        per_page,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SegmentFollowUpRequest {
    pub message: String,
    #[serde(default, deserialize_with = "crate::models::de_parse_mode")]
    pub parse_mode: Option<String>,
}

/// Follow-up segmen: kirim pesan ke semua anggota via infrastruktur
/// broadcast v1.1 (maks 1000 penerima). Pengiriman berjalan di background;
/// respons mengembalikan snapshot awal (sent/failed 0, status running) +
/// `broadcast_id` untuk polling `GET /api/messages/broadcast/:id`.
/// Anggota dari beberapa bot dikelompokkan per bot (satu broadcast per bot).
pub async fn segment_follow_up(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SegmentFollowUpRequest>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    if req.message.trim().is_empty() {
        return Err(AppError::Validation("message tidak boleh kosong".to_string()));
    }
    if let Some(mode) = &req.parse_mode {
        if !matches!(mode.as_str(), "Markdown" | "MarkdownV2" | "HTML") {
            return Err(AppError::Validation(
                "parse_mode harus Markdown, MarkdownV2, atau HTML".to_string(),
            ));
        }
    }
    let seg = fetch_segment(&state, &id).await?;
    let criteria = parse_criteria(&serde_json::from_str::<serde_json::Value>(&seg.criteria).unwrap_or(json!({})))?;
    let total_matched =
        segment_member_count(&state.pool, &criteria, seg.bot_id.as_deref()).await?;
    if total_matched == 0 {
        return Ok(Envelope::ok(json!({
            "total": 0,
            "sent": 0,
            "failed": 0,
            "broadcast_id": null,
            "broadcast_ids": [],
            "status": "done",
        })));
    }
    // Maks 1000 penerima (batas broadcast v1.1).
    let capped = total_matched > 1000;
    let members = segment_members(
        &state.pool,
        &criteria,
        seg.bot_id.as_deref(),
        1000,
        0,
    )
    .await?;
    // Kelompokkan per bot (urut kemunculan pertama).
    let mut by_bot: Vec<(String, Vec<i64>)> = Vec::new();
    for m in &members {
        match by_bot.iter_mut().find(|(b, _)| b == &m.bot_id) {
            Some((_, chats)) => chats.push(m.telegram_user_id),
            None => by_bot.push((m.bot_id.clone(), vec![m.telegram_user_id])),
        }
    }
    let mut broadcast_ids: Vec<String> = Vec::new();
    for (bot_id, chat_ids) in by_bot {
        let bc_req = BroadcastRequest {
            bot_id,
            text: req.message.clone(),
            parse_mode: req.parse_mode.clone(),
            recipients: chat_ids
                .into_iter()
                .map(|c| BroadcastRecipient { chat_id: c })
                .collect(),
        };
        let resp = create_broadcast(State(state.clone()), Json(bc_req)).await?;
        if let Some(bid) = resp
            .data
            .as_ref()
            .and_then(|d| d.get("broadcast_id"))
            .and_then(|v| v.as_str())
        {
            broadcast_ids.push(bid.to_string());
        }
    }
    // Snapshot status awal dari registry (biasanya running, 0/0).
    let (mut sent, mut failed) = (0i64, 0i64);
    {
        let registry = state.broadcasts.lock().unwrap();
        for bid in &broadcast_ids {
            if let Some(entry) = registry.get(bid) {
                sent += entry.sent;
                failed += entry.failed;
            }
        }
    }
    let total = members.len() as i64;
    Ok(Envelope::ok(json!({
        "total": total,
        "matched": total_matched,
        "capped_at_1000": capped,
        "sent": sent,
        "failed": failed,
        "broadcast_id": broadcast_ids.first(),
        "broadcast_ids": broadcast_ids,
        "status": "running",
    })))
}

// ---------------------------------------------------------------------------
// Timeline pengguna
// ---------------------------------------------------------------------------

fn truncate_summary(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push('…');
    }
    out
}

pub async fn get_user_timeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<serde_json::Value>>> {
    let user = fetch_user(&state, &id).await?;
    let mut items: Vec<serde_json::Value> = Vec::new();

    let messages = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE bot_id = ? AND from_user_id = ? \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(&user.bot_id)
    .bind(user.telegram_user_id)
    .fetch_all(&state.pool)
    .await?;
    for m in messages {
        let summary = match m.content.as_deref().filter(|c| !c.trim().is_empty()) {
            Some(c) => truncate_summary(c, 120),
            None => format!("[{}]", m.message_type),
        };
        items.push(json!({
            "kind": "message",
            "created_at": m.created_at,
            "summary": summary,
            "data": {"message_id": m.id, "direction": m.direction, "message_type": m.message_type, "status": m.status},
        }));
    }

    let orders = sqlx::query_as::<_, Order>(
        "SELECT * FROM orders WHERE bot_id = ? AND CAST(chat_id AS INTEGER) = ? \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(&user.bot_id)
    .bind(user.telegram_user_id)
    .fetch_all(&state.pool)
    .await?;
    for o in orders {
        items.push(json!({
            "kind": "order",
            "created_at": o.created_at,
            "summary": format!("Order {} — {} {}", o.status, o.amount, o.currency),
            "data": {"order_id": o.id, "status": o.status, "amount": o.amount, "currency": o.currency, "product_id": o.product_id},
        }));
    }

    let notes = sqlx::query_as::<_, UserNote>(
        "SELECT * FROM user_notes WHERE user_id = ? ORDER BY created_at DESC LIMIT 50",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await?;
    for n in notes {
        items.push(json!({
            "kind": "note",
            "created_at": n.created_at,
            "summary": truncate_summary(&n.note, 200),
            "data": {"note_id": n.id},
        }));
    }

    let tag_rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT t.id, t.name, a.assigned_at FROM user_tags t \
         JOIN user_tag_assignments a ON a.tag_id = t.id \
         WHERE a.user_id = ? ORDER BY a.assigned_at DESC LIMIT 50",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await?;
    for (tag_id, tag_name, assigned_at) in tag_rows {
        items.push(json!({
            "kind": "tag",
            "created_at": assigned_at,
            "summary": format!("Tag ditambahkan: {tag_name}"),
            "data": {"tag_id": tag_id, "tag_name": tag_name},
        }));
    }

    // Kronologis menurun (string RFC3339 terurut leksikografis).
    items.sort_by(|a, b| {
        b.get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .cmp(a.get("created_at").and_then(|v| v.as_str()).unwrap_or(""))
    });
    items.truncate(50);
    Ok(Envelope::ok(json!({"items": items})))
}
