//! Pesan: list, kirim (text/media/location/contact/media group),
//! kirim media multipart (upload file), retry, history.

use axum::{
    extract::{Multipart, Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::bot::get_bot_token;
use crate::bot::telegram::{SendMediaParams, SendMessageParams};
use crate::error::{AppError, AppResult, Envelope};
use crate::models::{new_id, now_rfc3339, Message, Paginated, Pagination};
use crate::services::log_activity;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// DTO + validasi (pure, diuji unit)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub bot_id: String,
    #[serde(deserialize_with = "crate::models::de_chat_id")]
    pub chat_id: i64,
    pub text: Option<String>,
    /// photo | document | video | audio | animation | location | contact.
    /// Alias `message_type` dipakai frontend; nilai "text"/"none" = teks biasa.
    #[serde(
        default,
        alias = "message_type",
        deserialize_with = "crate::models::de_media_type"
    )]
    pub media_type: Option<String>,
    pub media_url: Option<String>,
    pub media_file_id: Option<String>,
    pub media_group: Option<Vec<serde_json::Value>>,
    #[serde(default, deserialize_with = "crate::models::de_parse_mode")]
    pub parse_mode: Option<String>,
    /// Alias `keyboard` dipakai frontend (payload inline/reply keyboard).
    #[serde(default, alias = "keyboard")]
    pub reply_markup: Option<serde_json::Value>,
    pub caption: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub phone_number: Option<String>,
    pub contact_first_name: Option<String>,
    /// Media dari file picker frontend: `{type, name, data_url}` (base64).
    pub media: Option<MediaUpload>,
}

/// Satu file media yang dikirim frontend sebagai data URL base64.
#[derive(Debug, Deserialize)]
pub struct MediaUpload {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub name: Option<String>,
    pub data_url: Option<String>,
}

/// Jenis media efektif: `media_type` eksplisit, lalu `media.type`,
/// lalu "document" bila ada data URL tanpa jenis yang jelas.
pub fn effective_media_type(req: &SendMessageRequest) -> Option<String> {
    if let Some(mt) = &req.media_type {
        return Some(mt.clone());
    }
    if let Some(kind) = req.media.as_ref().and_then(|m| m.kind.as_deref()) {
        let k = kind.trim();
        if matches!(
            k,
            "photo" | "document" | "video" | "audio" | "animation" | "location" | "contact"
        ) {
            return Some(k.to_string());
        }
    }
    if req
        .media
        .as_ref()
        .and_then(|m| m.data_url.as_deref())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        return Some("document".to_string());
    }
    None
}

/// Dekode data URL (`data:<mime>;base64,<data>`) atau base64 mentah.
pub fn decode_data_url(data_url: &str) -> AppResult<Vec<u8>> {
    use base64::Engine;
    let b64 = data_url.splitn(2, ',').nth(1).unwrap_or(data_url);
    base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| AppError::Validation(format!("data media tidak valid (base64): {e}")))
}

fn method_and_field(media_type: &str) -> (&'static str, &'static str) {
    match media_type {
        "photo" => ("sendPhoto", "photo"),
        "video" => ("sendVideo", "video"),
        "audio" => ("sendAudio", "audio"),
        "animation" => ("sendAnimation", "animation"),
        _ => ("sendDocument", "document"),
    }
}

pub fn validate_send_message(req: &SendMessageRequest) -> AppResult<()> {
    if req.bot_id.trim().is_empty() {
        return Err(AppError::Validation("bot_id tidak boleh kosong".to_string()));
    }
    if req.chat_id == 0 {
        return Err(AppError::Validation("chat_id tidak boleh kosong".to_string()));
    }
    if let Some(mode) = &req.parse_mode {
        if !matches!(mode.as_str(), "Markdown" | "MarkdownV2" | "HTML") {
            return Err(AppError::Validation(
                "parse_mode harus Markdown, MarkdownV2, atau HTML".to_string(),
            ));
        }
    }
    if let Some(group) = &req.media_group {
        if group.is_empty() || group.len() > 10 {
            return Err(AppError::Validation(
                "media_group harus berisi 1–10 item".to_string(),
            ));
        }
        return Ok(());
    }
    let has_data_url = req
        .media
        .as_ref()
        .and_then(|m| m.data_url.as_deref())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    match effective_media_type(req).as_deref() {
        Some("location") => {
            if req.latitude.is_none() || req.longitude.is_none() {
                return Err(AppError::Validation(
                    "location membutuhkan latitude dan longitude".to_string(),
                ));
            }
            if !( -90.0..=90.0).contains(&req.latitude.unwrap())
                || !(-180.0..=180.0).contains(&req.longitude.unwrap())
            {
                return Err(AppError::Validation(
                    "latitude harus -90..90 dan longitude -180..180".to_string(),
                ));
            }
        }
        Some("contact") => {
            if req
                .phone_number
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
                || req
                    .contact_first_name
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
            {
                return Err(AppError::Validation(
                    "contact membutuhkan phone_number dan contact_first_name".to_string(),
                ));
            }
        }
        Some("photo") | Some("document") | Some("video") | Some("audio") | Some("animation") => {
            let has_media = has_data_url
                || req
                    .media_url
                    .as_deref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false)
                || req
                    .media_file_id
                    .as_deref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
            if !has_media {
                return Err(AppError::Validation(
                    "media membutuhkan media_url, media_file_id, atau media.data_url".to_string(),
                ));
            }
        }
        Some(other) => {
            return Err(AppError::Validation(format!(
                "media_type tidak dikenal: '{other}'"
            )));
        }
        None => {
            if req.text.as_deref().map(str::trim).unwrap_or("").is_empty() {
                return Err(AppError::Validation(
                    "text tidak boleh kosong untuk pesan teks".to_string(),
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize, Default)]
pub struct ListMessagesQuery {
    pub bot_id: Option<String>,
    pub chat_id: Option<i64>,
    pub direction: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    pub pagination: Pagination,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn save_outbound_message(
    state: &AppState,
    bot_id: &str,
    chat_id: i64,
    message_type: &str,
    content: Option<String>,
    media_file_id: Option<String>,
    status: &str,
    telegram_message_id: Option<i64>,
    parse_mode: Option<&str>,
    error_message: Option<String>,
) -> AppResult<Message> {
    let id = new_id();
    let now = now_rfc3339();
    sqlx::query(
        "INSERT INTO messages (id, bot_id, chat_id, telegram_message_id, direction, \
         message_type, content, media_file_id, status, parse_mode, error_message, created_at, updated_at) \
         VALUES (?, ?, ?, ?, 'outbound', ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(bot_id)
    .bind(chat_id)
    .bind(telegram_message_id)
    .bind(message_type)
    .bind(content.as_deref())
    .bind(media_file_id.as_deref())
    .bind(status)
    .bind(parse_mode)
    .bind(error_message.as_deref())
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;
    let msg = sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    Ok(msg)
}

// ---------------------------------------------------------------------------
// List + history
// ---------------------------------------------------------------------------

pub async fn list_messages(
    State(state): State<AppState>,
    Query(q): Query<ListMessagesQuery>,
) -> AppResult<Json<Envelope<Paginated<Message>>>> {
    let (page, per_page) = q.pagination.normalized();
    let offset = q.pagination.offset();
    let mut qb = sqlx::QueryBuilder::new("SELECT * FROM messages WHERE 1=1");
    if let Some(b) = &q.bot_id {
        qb.push(" AND bot_id = ").push_bind(b);
    }
    if let Some(c) = q.chat_id {
        qb.push(" AND chat_id = ").push_bind(c);
    }
    if let Some(d) = &q.direction {
        qb.push(" AND direction = ").push_bind(d);
    }
    if let Some(s) = &q.status {
        qb.push(" AND status = ").push_bind(s);
    }
    qb.push(" ORDER BY created_at DESC LIMIT ").push_bind(per_page);
    qb.push(" OFFSET ").push_bind(offset);
    let items = qb
        .build_query_as::<Message>()
        .fetch_all(&state.pool)
        .await?;

    let mut count_qb = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM messages WHERE 1=1");
    if let Some(b) = &q.bot_id {
        count_qb.push(" AND bot_id = ").push_bind(b);
    }
    if let Some(c) = q.chat_id {
        count_qb.push(" AND chat_id = ").push_bind(c);
    }
    if let Some(d) = &q.direction {
        count_qb.push(" AND direction = ").push_bind(d);
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

pub async fn message_history(
    State(state): State<AppState>,
    Query(q): Query<ListMessagesQuery>,
) -> AppResult<Json<Envelope<Paginated<Message>>>> {
    // History = list terbaru (maks 50) untuk bot/chat tertentu.
    let mut q = q;
    if q.pagination.per_page.is_none() {
        q.pagination.per_page = Some(50);
    }
    list_messages(State(state), Query(q)).await
}

// ---------------------------------------------------------------------------
// Kirim pesan
// ---------------------------------------------------------------------------

pub async fn send_message(
    State(state): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> AppResult<Json<Envelope<Message>>> {
    validate_send_message(&req)?;
    let token = get_bot_token(&state, &req.bot_id).await?;
    let parse_mode = req.parse_mode.clone();

    // Dispatch berdasarkan jenis kiriman.
    let send_result: Result<(String, Option<String>, Option<i64>), AppError> = async {
        // Media dari frontend (data URL base64) — dekode lalu upload multipart.
        if let Some(data_url) = req
            .media
            .as_ref()
            .and_then(|m| m.data_url.clone())
            .filter(|s| !s.trim().is_empty())
        {
            let bytes = decode_data_url(&data_url)?;
            if bytes.len() > 50 * 1024 * 1024 {
                return Err(AppError::Validation(
                    "file terlalu besar (maks 50 MB, batas Bot API)".to_string(),
                ));
            }
            let mt = effective_media_type(&req).unwrap_or_else(|| "document".to_string());
            let (method, field) = method_and_field(&mt);
            let filename = req
                .media
                .as_ref()
                .and_then(|m| m.name.clone())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("telecore-media.{mt}"));
            let caption = req.caption.clone().or_else(|| req.text.clone());
            let sent = state
                .telegram
                .send_media_bytes(&token, method, field, req.chat_id, bytes, filename, caption.clone())
                .await?;
            return Ok((mt, caption, Some(sent.message_id)));
        }
        if let Some(group) = &req.media_group {
            let sent = state
                .telegram
                .send_media_group(&token, req.chat_id, group)
                .await?;
            let first_id = sent.first().map(|m| m.message_id);
            return Ok((
                "media_group".to_string(),
                Some(format!("Media group ({} item)", group.len())),
                first_id,
            ));
        }
        match effective_media_type(&req).as_deref() {
            Some("location") => {
                let sent = state
                    .telegram
                    .send_location(
                        &token,
                        req.chat_id,
                        req.latitude.unwrap(),
                        req.longitude.unwrap(),
                    )
                    .await?;
                Ok((
                    "location".to_string(),
                    Some(format!("{}, {}", req.latitude.unwrap(), req.longitude.unwrap())),
                    Some(sent.message_id),
                ))
            }
            Some("contact") => {
                let sent = state
                    .telegram
                    .send_contact(
                        &token,
                        req.chat_id,
                        req.phone_number.as_deref().unwrap(),
                        req.contact_first_name.as_deref().unwrap(),
                    )
                    .await?;
                Ok((
                    "contact".to_string(),
                    Some(req.contact_first_name.clone().unwrap_or_default()),
                    Some(sent.message_id),
                ))
            }
            Some(mt @ ("photo" | "document" | "video" | "audio" | "animation")) => {
                let media = req
                    .media_file_id
                    .clone()
                    .or_else(|| req.media_url.clone())
                    .unwrap();
                let params = SendMediaParams {
                    chat_id: req.chat_id,
                    media,
                    caption: req.caption.clone(),
                    parse_mode: parse_mode.clone(),
                    reply_markup: req.reply_markup.clone(),
                };
                let sent = match mt {
                    "photo" => state.telegram.send_photo(&token, &params).await?,
                    "document" => state.telegram.send_document(&token, &params).await?,
                    "video" => state.telegram.send_video(&token, &params).await?,
                    "audio" => state.telegram.send_audio(&token, &params).await?,
                    _ => state.telegram.send_animation(&token, &params).await?,
                };
                Ok((
                    mt.to_string(),
                    req.caption.clone(),
                    Some(sent.message_id),
                ))
            }
            _ => {
                let sent = state
                    .telegram
                    .send_message(&token, &SendMessageParams {
                        chat_id: req.chat_id,
                        text: req.text.clone().unwrap_or_default(),
                        parse_mode: parse_mode.clone(),
                        reply_markup: req.reply_markup.clone(),
                        ..Default::default()
                    })
                    .await?;
                Ok((
                    "text".to_string(),
                    req.text.clone(),
                    Some(sent.message_id),
                ))
            }
        }
    }
    .await;

    match send_result {
        Ok((message_type, content, tg_id)) => {
            let saved = save_outbound_message(
                &state,
                &req.bot_id,
                req.chat_id,
                &message_type,
                content,
                req.media_file_id.clone(),
                "sent",
                tg_id,
                parse_mode.as_deref(),
                None,
            )
            .await?;
            state.hub.publish(
                "message",
                json!({"bot_id": req.bot_id, "chat_id": req.chat_id, "direction": "outbound", "message_type": message_type}),
            );
            Ok(Envelope::ok(saved))
        }
        Err(e) => {
            // Simpan juga pesan gagal agar terlihat di history/analytics.
            let message_type = effective_media_type(&req).unwrap_or_else(|| "text".to_string());
            let _ = save_outbound_message(
                &state,
                &req.bot_id,
                req.chat_id,
                &message_type,
                req.text.clone(),
                req.media_file_id.clone(),
                "failed",
                None,
                parse_mode.as_deref(),
                Some(e.to_string()),
            )
            .await;
            Err(e)
        }
    }
}

/// Kirim media dengan upload file mentah (multipart/form-data).
/// Field: bot_id, chat_id, media_type (photo/document/video/audio/animation),
/// caption (opsional), file (biner).
pub async fn send_media_multipart(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> AppResult<Json<Envelope<Message>>> {
    let mut bot_id: Option<String> = None;
    let mut chat_id: Option<i64> = None;
    let mut media_type: Option<String> = None;
    let mut caption: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename = "file".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("multipart tidak valid: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                if let Some(f) = field.file_name() {
                    filename = f.to_string();
                }
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::Validation(format!("gagal membaca file: {e}")))?;
                if bytes.len() > 50 * 1024 * 1024 {
                    return Err(AppError::Validation(
                        "file terlalu besar (maks 50 MB, batas Bot API)".to_string(),
                    ));
                }
                file_bytes = Some(bytes.to_vec());
            }
            "bot_id" => {
                bot_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::Validation(format!("bot_id tidak valid: {e}")))?,
                );
            }
            "chat_id" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::Validation(format!("chat_id tidak valid: {e}")))?;
                chat_id = text.trim().parse().ok();
            }
            "media_type" => {
                media_type = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::Validation(format!("media_type tidak valid: {e}")))?,
                );
            }
            "caption" => {
                caption = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::Validation(format!("caption tidak valid: {e}")))?,
                );
            }
            _ => {}
        }
    }

    let bot_id = bot_id
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| AppError::Validation("bot_id tidak boleh kosong".to_string()))?;
    let chat_id = chat_id
        .filter(|c| *c != 0)
        .ok_or_else(|| AppError::Validation("chat_id tidak boleh kosong".to_string()))?;
    let media_type = media_type.unwrap_or_else(|| "document".to_string());
    let (method, field_name) = match media_type.as_str() {
        "photo" => ("sendPhoto", "photo"),
        "video" => ("sendVideo", "video"),
        "audio" => ("sendAudio", "audio"),
        "animation" => ("sendAnimation", "animation"),
        "document" => ("sendDocument", "document"),
        other => {
            return Err(AppError::Validation(format!(
                "media_type tidak dikenal: '{other}'"
            )))
        }
    };
    let bytes = file_bytes
        .ok_or_else(|| AppError::Validation("field 'file' wajib diisi".to_string()))?;

    let token = get_bot_token(&state, &bot_id).await?;
    let sent = state
        .telegram
        .send_media_bytes(
            &token,
            method,
            field_name,
            chat_id,
            bytes,
            filename,
            caption.clone(),
        )
        .await;
    match sent {
        Ok(msg) => {
            let saved = save_outbound_message(
                &state,
                &bot_id,
                chat_id,
                &media_type,
                caption,
                None,
                "sent",
                Some(msg.message_id),
                None,
                None,
            )
            .await?;
            Ok(Envelope::ok(saved))
        }
        Err(e) => {
            let _ = save_outbound_message(
                &state,
                &bot_id,
                chat_id,
                &media_type,
                caption,
                None,
                "failed",
                None,
                None,
                Some(e.to_string()),
            )
            .await;
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Retry
// ---------------------------------------------------------------------------

pub async fn retry_message(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Envelope<Message>>> {
    let msg = sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("pesan {id} tidak ditemukan")))?;
    if msg.direction != "outbound" {
        return Err(AppError::Validation(
            "hanya pesan outbound yang bisa di-retry".to_string(),
        ));
    }
    if msg.status == "sent" {
        return Err(AppError::Validation("pesan sudah terkirim".to_string()));
    }
    let token = get_bot_token(&state, &msg.bot_id).await?;
    let text = msg.content.clone().unwrap_or_default();
    if msg.message_type != "text" || text.is_empty() {
        return Err(AppError::Validation(
            "retry hanya didukung untuk pesan teks (konten media tidak tersimpan)".to_string(),
        ));
    }
    let sent = state
        .telegram
        .send_message(&token, &SendMessageParams {
            chat_id: msg.chat_id,
            text,
            parse_mode: msg.parse_mode.clone(),
            ..Default::default()
        })
        .await;
    match sent {
        Ok(tg) => {
            sqlx::query(
                "UPDATE messages SET status = 'sent', telegram_message_id = ?, error_message = NULL, updated_at = ? WHERE id = ?",
            )
            .bind(tg.message_id)
            .bind(now_rfc3339())
            .bind(&id)
            .execute(&state.pool)
            .await?;
        }
        Err(e) => {
            sqlx::query(
                "UPDATE messages SET status = 'failed', error_message = ?, updated_at = ? WHERE id = ?",
            )
            .bind(e.to_string())
            .bind(now_rfc3339())
            .bind(&id)
            .execute(&state.pool)
            .await?;
            return Err(e);
        }
    }
    let updated = sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    log_activity(
        &state,
        Some(&msg.bot_id),
        "message_retried",
        Some("message"),
        Some(&id),
        json!({"chat_id": msg.chat_id}),
        "info",
    )
    .await;
    Ok(Envelope::ok(updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_req() -> SendMessageRequest {
        SendMessageRequest {
            bot_id: "bot-1".to_string(),
            chat_id: 123,
            text: Some("halo".to_string()),
            media_type: None,
            media_url: None,
            media_file_id: None,
            media_group: None,
            parse_mode: None,
            reply_markup: None,
            caption: None,
            latitude: None,
            longitude: None,
            phone_number: None,
            contact_first_name: None,
            media: None,
        }
    }

    #[test]
    fn validation_rejects_empty_bot_id_and_chat_id() {
        let mut req = base_req();
        req.bot_id = "".to_string();
        assert!(validate_send_message(&req).is_err());
        let mut req = base_req();
        req.chat_id = 0; // chat_id kosong
        assert!(validate_send_message(&req).is_err());
    }

    #[test]
    fn validation_rejects_empty_text_without_media() {
        let mut req = base_req();
        req.text = Some("   ".to_string());
        assert!(validate_send_message(&req).is_err());
    }

    #[test]
    fn validation_rejects_bad_parse_mode() {
        let mut req = base_req();
        req.parse_mode = Some("BBCode".to_string());
        assert!(validate_send_message(&req).is_err());
    }

    #[test]
    fn validation_accepts_text_and_media() {
        assert!(validate_send_message(&base_req()).is_ok());
        let mut req = base_req();
        req.text = None;
        req.media_type = Some("photo".to_string());
        req.media_url = Some("https://example.com/a.jpg".to_string());
        assert!(validate_send_message(&req).is_ok());
    }

    #[test]
    fn frontend_text_payload_deserializes() {
        // Persis seperti yang dikirim MessageComposer React.
        let req: SendMessageRequest = serde_json::from_str(
            r#"{"bot_id":"bot-1","chat_id":"123456789","message_type":"text","text":"halo","parse_mode":"none","keyboard":{"inline_keyboard":[[{"text":"OK","callback_data":"ok"}]]}}"#,
        )
        .expect("payload frontend harus bisa di-parse");
        assert_eq!(req.chat_id, 123456789);
        assert_eq!(req.media_type, None);
        assert_eq!(req.parse_mode, None);
        assert!(req.reply_markup.is_some());
        assert!(validate_send_message(&req).is_ok());
    }

    #[test]
    fn frontend_media_payload_deserializes() {
        let req: SendMessageRequest = serde_json::from_str(
            r#"{"bot_id":"bot-1","chat_id":-100123,"message_type":"photo","text":"keterangan","media":{"type":"photo","name":"a.png","data_url":"data:image/png;base64,aGk="}}"#,
        )
        .expect("payload media frontend harus bisa di-parse");
        assert_eq!(req.chat_id, -100123);
        assert_eq!(effective_media_type(&req).as_deref(), Some("photo"));
        assert!(validate_send_message(&req).is_ok());
    }

    #[test]
    fn chat_id_rejects_username() {
        let res: Result<SendMessageRequest, _> = serde_json::from_str(
            r#"{"bot_id":"bot-1","chat_id":"@username","text":"halo"}"#,
        );
        assert!(res.is_err());
    }

    #[test]
    fn decode_data_url_round_trip() {
        let bytes = decode_data_url("data:image/png;base64,aGk=").unwrap();
        assert_eq!(bytes, b"hi");
        assert!(decode_data_url("data:image/png;base64,!!!").is_err());
    }
}
