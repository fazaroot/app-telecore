//! Pemrosesan update Telegram — dipakai BERSAMA oleh webhook dan long polling
//! agar perilakunya identik:
//!
//! 1. Abaikan pesan dari bot (loop guard, termasuk bot sendiri).
//! 2. Upsert user & chat, simpan pesan masuk.
//! 3. Moderasi grup dasar (keyword/link/flood) — eksekusi HANYA bila bot admin,
//!    bila bukan admin hanya dicatat di activity log.
//! 4. Auto-reply command kustom + jalankan automation engine.
//! 5. Broadcast event WebSocket.

use crate::automation::{render_template, run_workflows};
use crate::bot::get_bot_token;
use crate::bot::telegram::{SendMessageParams, TelegramMessage, Update};
use crate::models::{new_id, now_rfc3339, BotChat, BotCommand, GroupSettings};
use crate::services::ai::{automation_handled, maybe_ai_reply};
use crate::services::group_moderation::{
    detect_text_violations, enforcement_active, flood_count, record_moderation, Violation,
};
use crate::services::outgoing_webhooks::dispatch_outgoing_event;
use crate::services::template_vars::{render_template_vars, TemplateVars};
use crate::state::AppState;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration as StdDuration, Instant};

// ---------------------------------------------------------------------------
// Activity log helper (dipakai banyak modul)
// ---------------------------------------------------------------------------

pub async fn log_activity(
    state: &AppState,
    bot_id: Option<&str>,
    action: &str,
    entity_type: Option<&str>,
    entity_id: Option<&str>,
    details: serde_json::Value,
    level: &str,
) {
    // Redaksi terpusat sebelum disimpan: details JSON (termasuk pesan
    // error dari pemanggil mana pun) tidak boleh mengandung token bot.
    let action = crate::security::redact::redact_secrets(action);
    let entity_type = entity_type.map(crate::security::redact::redact_secrets);
    let entity_id = entity_id.map(crate::security::redact::redact_secrets);
    let mut details = details;
    crate::security::redact::redact_json_value(&mut details);
    let _ = sqlx::query(
        "INSERT INTO activity_logs (id, bot_id, action, entity_type, entity_id, details, level, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(bot_id)
    .bind(&action)
    .bind(entity_type.as_deref())
    .bind(entity_id.as_deref())
    .bind(details.to_string())
    .bind(level)
    .bind(now_rfc3339())
    .execute(&state.pool)
    .await;
}

// ---------------------------------------------------------------------------
// Pemrosesan update
// ---------------------------------------------------------------------------

pub async fn process_update(state: &AppState, bot_id: &str, update: &Update) {
    // v1.2: pre-checkout query pembayaran — jawab ok=true agar user bisa
    // melanjutkan pembayaran (tidak ada validasi harga tambahan di v1.2).
    if let Some(pcq) = &update.pre_checkout_query {
        handle_pre_checkout_query(state, bot_id, pcq).await;
        return;
    }

    let Some(msg) = update
        .message
        .as_ref()
        .or(update.edited_message.as_ref())
        .or(update.channel_post.as_ref())
    else {
        return;
    };

    // Loop guard: abaikan pesan dari bot (termasuk bot sendiri).
    if msg.from.as_ref().map(|u| u.is_bot).unwrap_or(false) {
        return;
    }

    let now = now_rfc3339();
    let chat = &msg.chat;
    let from = msg.from.as_ref();
    let text = msg.text.clone().or_else(|| msg.caption.clone()).unwrap_or_default();

    // --- Upsert user ---
    let mut is_new_user = false;
    if let Some(user) = from {
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM bot_users WHERE bot_id = ? AND telegram_user_id = ?",
        )
        .bind(bot_id)
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await
        .ok()
        .flatten();
        is_new_user = existing.is_none();
        let _ = sqlx::query(
            "INSERT INTO bot_users (id, bot_id, telegram_user_id, username, first_name, last_name, \
             language_code, is_bot, last_seen_at, message_count, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, 1, ?, ?) \
             ON CONFLICT(bot_id, telegram_user_id) DO UPDATE SET \
             username = excluded.username, first_name = excluded.first_name, \
             last_name = excluded.last_name, language_code = excluded.language_code, \
             last_seen_at = excluded.last_seen_at, \
             message_count = message_count + 1, updated_at = excluded.updated_at",
        )
        .bind(new_id())
        .bind(bot_id)
        .bind(user.id)
        .bind(user.username.as_deref())
        .bind(user.first_name.as_str())
        .bind(user.last_name.as_deref())
        .bind(user.language_code.as_deref())
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await;
    }

    // --- Upsert chat ---
    let _ = sqlx::query(
        "INSERT INTO bot_chats (id, bot_id, chat_id, type, title, username, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(bot_id, chat_id) DO UPDATE SET \
         title = excluded.title, username = excluded.username, updated_at = excluded.updated_at",
    )
    .bind(new_id())
    .bind(bot_id)
    .bind(chat.id)
    .bind(chat.chat_type.as_str())
    .bind(chat.title.as_deref())
    .bind(chat.username.as_deref())
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await;

    // --- Simpan pesan masuk (v1.2: is_read=0 = belum dibaca di inbox) ---
    let message_type = if msg.successful_payment.is_some() {
        "payment"
    } else if msg.photo.is_some() {
        "photo"
    } else if msg.new_chat_members.is_some() {
        "new_chat_members"
    } else {
        "text"
    };
    let _ = sqlx::query(
        "INSERT INTO messages (id, bot_id, chat_id, telegram_message_id, direction, \
         message_type, content, status, from_user_id, is_read, created_at, updated_at) \
         VALUES (?, ?, ?, ?, 'inbound', ?, ?, 'received', ?, 0, ?, ?)",
    )
    .bind(new_id())
    .bind(bot_id)
    .bind(chat.id)
    .bind(msg.message_id)
    .bind(message_type)
    .bind(if text.is_empty() { None } else { Some(text.clone()) })
    .bind(from.map(|u| u.id))
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await;

    // --- Outgoing webhook: message.inbound (non-blocking, spawn) ---
    dispatch_outgoing_event(
        state,
        "message.inbound",
        Some(bot_id),
        serde_json::json!({
            "chat_id": chat.id,
            "from_user_id": from.map(|u| u.id),
            "text": text,
            "message_type": message_type,
            "telegram_message_id": msg.message_id,
        }),
    )
    .await;

    // --- Broadcast WebSocket ---
    state.hub.publish(
        "message",
        serde_json::json!({
            "bot_id": bot_id,
            "chat_id": chat.id,
            "from_user_id": from.map(|u| u.id),
            "text": text,
            "message_type": message_type,
        }),
    );

    // --- v1.2: successful_payment — tandai order lunas, lalu berhenti
    // (tidak ada automation / AI untuk pesan pembayaran).
    if let Some(payment) = &msg.successful_payment {
        handle_successful_payment(state, bot_id, payment).await;
        return;
    }

    // --- Konteks automation ---
    let context = serde_json::json!({
        "text": text,
        "chat_id": chat.id,
        "chat_type": chat.chat_type,
        "user_id": from.map(|u| u.id),
        "username": from.and_then(|u| u.username.clone()),
        "first_name": from.map(|u| u.first_name.clone()),
        "message_id": msg.message_id,
    });

    // --- User baru ---
    if is_new_user {
        if let Some(user) = from {
            log_activity(
                state,
                Some(bot_id),
                "new_user",
                Some("bot_user"),
                Some(&user.id.to_string()),
                serde_json::json!({"username": user.username, "first_name": user.first_name}),
                "info",
            )
            .await;
        }
        run_workflows(state, bot_id, "new_user", context.clone()).await;
    }

    // --- User bergabung ke grup (welcome message) ---
    if let Some(members) = &msg.new_chat_members {
        for member in members {
            if member.is_bot {
                continue;
            }
            log_activity(
                state,
                Some(bot_id),
                "user_joined_group",
                Some("bot_chat"),
                Some(&chat.id.to_string()),
                serde_json::json!({"user_id": member.id, "first_name": member.first_name}),
                "info",
            )
            .await;
            let join_ctx = serde_json::json!({
                "text": text,
                "chat_id": chat.id,
                "chat_type": chat.chat_type,
                "user_id": member.id,
                "username": member.username,
                "first_name": member.first_name,
                "message_id": msg.message_id,
            });
            run_workflows(state, bot_id, "user_joined_group", join_ctx).await;
            send_welcome_if_configured(state, bot_id, chat.id, member.first_name.as_str()).await;
        }
    }

    // --- Moderasi grup dasar ---
    if chat.chat_type == "group" || chat.chat_type == "supergroup" {
        moderate_group_message(state, bot_id, msg, &text).await;
    }

    // --- Command kustom ---
    if text.starts_with('/') {
        handle_command(state, bot_id, msg, &text).await;
        run_workflows(state, bot_id, "command", context.clone()).await;
    }

    // --- Trigger umum ---
    run_workflows(state, bot_id, "message_received", context.clone()).await;
    run_workflows(state, bot_id, "keyword", context).await;

    // --- v1.2: AI Smart Reply — hanya bila command/keyword automation
    // tidak menangani pesan ini (dan bukan command '/...').
    if !text.trim().is_empty()
        && !text.starts_with('/')
        && !automation_handled(state, bot_id, &text).await
    {
        maybe_ai_reply(
            state,
            bot_id,
            chat.id,
            &chat.chat_type,
            from.map(|u| u.id),
            msg.message_id,
            &text,
        )
        .await;
    }
}

// ---------------------------------------------------------------------------
// Pembayaran Stars (v1.2)
// ---------------------------------------------------------------------------

async fn handle_pre_checkout_query(
    state: &AppState,
    bot_id: &str,
    pcq: &serde_json::Value,
) {
    let Some(query_id) = pcq.get("id").and_then(|v| v.as_str()) else {
        return;
    };
    let Ok(token) = get_bot_token(state, bot_id).await else {
        return;
    };
    let _ = state
        .telegram
        .answer_pre_checkout_query(&token, query_id, true, None)
        .await;
    log_activity(
        state,
        Some(bot_id),
        "pre_checkout_answered",
        Some("order"),
        None,
        serde_json::json!({
            "pre_checkout_query_id": query_id,
            "invoice_payload": pcq.get("invoice_payload"),
            "total_amount": pcq.get("total_amount"),
            "currency": pcq.get("currency"),
        }),
        "info",
    )
    .await;
}

async fn handle_successful_payment(
    state: &AppState,
    bot_id: &str,
    payment: &serde_json::Value,
) {
    let invoice_payload = payment
        .get("invoice_payload")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if invoice_payload.is_empty() {
        return;
    }
    let tg_charge = payment
        .get("telegram_payment_charge_id")
        .and_then(|v| v.as_str());
    let provider_charge = payment
        .get("provider_payment_charge_id")
        .and_then(|v| v.as_str());
    let now = now_rfc3339();
    // Cari order berdasarkan invoice_payload persis (berisi product payload
    // + '#' + order id); fallback ke product payload saja.
    let order_id: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM orders WHERE bot_id = ? AND invoice_payload = ? AND status = 'pending'",
    )
    .bind(bot_id)
    .bind(invoice_payload)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let Some((order_id,)) = order_id else {
        log_activity(
            state,
            Some(bot_id),
            "payment_received_unknown_order",
            Some("order"),
            None,
            serde_json::json!({"invoice_payload": invoice_payload}),
            "warning",
        )
        .await;
        return;
    };
    let _ = sqlx::query(
        "UPDATE orders SET status = 'paid', paid_at = ?, \
         telegram_payment_charge_id = ?, provider_payment_charge_id = ? WHERE id = ?",
    )
    .bind(&now)
    .bind(tg_charge)
    .bind(provider_charge)
    .bind(&order_id)
    .execute(&state.pool)
    .await;
    log_activity(
        state,
        Some(bot_id),
        "payment_successful",
        Some("order"),
        Some(&order_id),
        serde_json::json!({
            "invoice_payload": invoice_payload,
            "total_amount": payment.get("total_amount"),
            "currency": payment.get("currency"),
            "telegram_payment_charge_id": tg_charge,
        }),
        "info",
    )
    .await;
}

async fn send_welcome_if_configured(state: &AppState, bot_id: &str, chat_id: i64, first_name: &str) {
    // v1.3: group_settings (welcome_enabled + welcome_message dengan
    // substitusi {{first_name}} / {{chat_title}}).
    let gs: Option<GroupSettings> =
        sqlx::query_as("SELECT * FROM group_settings WHERE chat_id = ?")
            .bind(chat_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    if let Some(gs) = gs {
        if gs.welcome_enabled != 0 {
            if let Some(welcome) = gs.welcome_message.as_deref() {
                if !welcome.trim().is_empty() {
                    let chat_row = sqlx::query_as::<_, BotChat>(
                        "SELECT * FROM bot_chats WHERE bot_id = ? AND chat_id = ?",
                    )
                    .bind(bot_id)
                    .bind(chat_id)
                    .fetch_optional(&state.pool)
                    .await
                    .ok()
                    .flatten();
                    let vars = TemplateVars::new(
                        Some(first_name.to_string()),
                        None,
                        chat_row.and_then(|c| c.title),
                    );
                    let text = render_template_vars(welcome, &vars);
                    if let Ok(token) = get_bot_token(state, bot_id).await {
                        let _ = state
                            .telegram
                            .send_message(&token, &SendMessageParams {
                                chat_id,
                                text,
                                ..Default::default()
                            })
                            .await;
                    }
                    return;
                }
            }
        }
    }
    // Legacy fallback: bot_chats.settings welcome_message.
    let chat_row = sqlx::query_as::<_, BotChat>(
        "SELECT * FROM bot_chats WHERE bot_id = ? AND chat_id = ?",
    )
    .bind(bot_id)
    .bind(chat_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let Some(chat_row) = chat_row else { return };
    let settings: serde_json::Value =
        serde_json::from_str(&chat_row.settings).unwrap_or(serde_json::json!({}));
    let Some(welcome) = settings.get("welcome_message").and_then(|v| v.as_str()) else {
        return;
    };
    if welcome.trim().is_empty() {
        return;
    }
    let text = render_template(
        welcome,
        &serde_json::json!({"first_name": first_name, "chat_id": chat_id}),
    );
    if let Ok(token) = get_bot_token(state, bot_id).await {
        let _ = state
            .telegram
            .send_message(&token, &SendMessageParams {
                chat_id,
                text,
                ..Default::default()
            })
            .await;
    }
}

/// Cache hasil cek `getChatMember` (admin/creator) max 60 detik per
/// (bot, chat, user) agar penegakan tidak membanjiri Bot API.
/// Cache proses-lokal; salah ketik status terburuk hanya bertahan 60 detik.
static ADMIN_CHECK_CACHE: std::sync::OnceLock<
    Mutex<HashMap<String, (bool, Instant)>>,
> = std::sync::OnceLock::new();

fn admin_cache() -> &'static Mutex<HashMap<String, (bool, Instant)>> {
    ADMIN_CHECK_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Apakah user ini admin/creator di chat (via getChatMember, cache 60 dtk).
/// Error jaringan → dianggap BUKAN admin (pemanggil yang memutuskan apakah
/// itu berarti "lewati penegakan" atau "lanjut dengan risiko gagal").
async fn is_chat_privileged(
    state: &AppState,
    token: &str,
    bot_id: &str,
    chat_id: i64,
    user_id: i64,
) -> bool {
    let key = format!("{bot_id}:{chat_id}:{user_id}");
    {
        let cache = admin_cache().lock().unwrap();
        if let Some((admin, at)) = cache.get(&key) {
            if at.elapsed() < StdDuration::from_secs(60) {
                return *admin;
            }
        }
    }
    let admin = match state.telegram.get_chat_member(token, chat_id, user_id).await {
        Ok(m) => m.status == "administrator" || m.status == "creator",
        Err(_) => false,
    };
    let mut cache = admin_cache().lock().unwrap();
    cache.insert(key, (admin, Instant::now()));
    admin
}

/// Ambil telegram_id bot sendiri (kolom bots.telegram_id, fallback getMe).
/// None bila Telegram tidak terjangkau — pemanggil memperlakukannya sebagai
/// "tidak bisa memverifikasi admin" (penegakan dilewati, bukan crash).
async fn own_telegram_id(state: &AppState, bot_id: &str, token: &str) -> Option<i64> {
    let row: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT telegram_id FROM bots WHERE id = ?")
            .bind(bot_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    if let Some((Some(id),)) = row {
        return Some(id);
    }
    match state.telegram.get_me(token).await {
        Ok(me) => {
            let _ = sqlx::query("UPDATE bots SET telegram_id = ? WHERE id = ?")
                .bind(me.id)
                .bind(bot_id)
                .execute(&state.pool)
                .await;
            Some(me.id)
        }
        Err(_) => None,
    }
}

/// Moderasi grup: bila ada aturan v1.3 yang aktif di `group_settings`,
/// pakai penegakan v1.3; bila tidak, fallback ke aturan legacy
/// (keyword_filter / link_filter / anti_spam dari `bot_chats.settings`).
async fn moderate_group_message(
    state: &AppState,
    bot_id: &str,
    msg: &TelegramMessage,
    text: &str,
) {
    // Pesan service (join/leave) tidak dimoderasi.
    if msg.new_chat_members.is_some() || msg.left_chat_member.is_some() {
        return;
    }
    let chat_id = msg.chat.id;
    let gs: Option<GroupSettings> =
        sqlx::query_as("SELECT * FROM group_settings WHERE chat_id = ?")
            .bind(chat_id)
            .fetch_optional(&state.pool)
            .await
            .ok()
            .flatten();
    if let Some(gs) = gs {
        if enforcement_active(&gs) {
            enforce_v13_group_message(state, bot_id, msg, text, &gs).await;
            return;
        }
    }
    moderate_group_message_legacy(state, bot_id, msg, text).await;
}

/// Penegakan v1.3: kata terlarang, block link, flood (hitung dari tabel
/// messages). Hanya untuk grup/supergrup; pesan privat tidak sampai sini.
/// - Admin/creator grup dan bot sendiri tidak pernah dibatasi.
/// - Satu pesan hanya diproses satu kali (processor sekali per update).
/// - Bila bot bukan admin: catat ke log, lewati penegakan, jangan crash.
async fn enforce_v13_group_message(
    state: &AppState,
    bot_id: &str,
    msg: &TelegramMessage,
    text: &str,
    gs: &GroupSettings,
) {
    let chat_id = msg.chat.id;
    let user_id = match msg.from.as_ref() {
        Some(u) => u.id,
        None => return,
    };

    let mut violations = detect_text_violations(text, gs);
    let mut flooded = false;
    if gs.anti_spam_enabled != 0 && gs.flood_limit > 0 {
        match flood_count(
            state,
            bot_id,
            chat_id,
            user_id,
            gs.flood_window_seconds,
        )
        .await
        {
            Ok(n) if n > gs.flood_limit => {
                flooded = true;
                violations.push(Violation::Flood);
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("Gagal menghitung flood di chat {chat_id}: {e}");
            }
        }
    }
    if violations.is_empty() {
        return;
    }
    let violation_names: Vec<&str> = violations
        .iter()
        .map(|v| match v {
            Violation::BannedWord => "banned_word",
            Violation::Link => "link",
            Violation::Flood => "flood",
        })
        .collect();

    let Ok(token) = get_bot_token(state, bot_id).await else {
        // Tanpa token tidak ada yang bisa dieksekusi; catat lalu lewati.
        record_moderation(
            state,
            bot_id,
            chat_id,
            Some(user_id),
            "logged_only_no_token",
            Some("Pelanggaran terdeteksi tetapi token bot tidak tersedia"),
            serde_json::json!({"chat_id": chat_id, "user_id": user_id, "message_id": msg.message_id, "violations": violation_names, "via": "auto"}),
        )
        .await;
        return;
    };
    let Some(bot_tg_id) = own_telegram_id(state, bot_id, &token).await else {
        // Telegram tidak terjangkau: tidak bisa verifikasi admin — catat,
        // lewati penegakan, jangan crash. Pesan tetap tersimpan normal.
        record_moderation(
            state,
            bot_id,
            chat_id,
            Some(user_id),
            "logged_only_telegram_unreachable",
            Some("Pelanggaran terdeteksi tetapi Telegram tidak terjangkau (tidak bisa cek admin)"),
            serde_json::json!({"chat_id": chat_id, "user_id": user_id, "message_id": msg.message_id, "violations": violation_names, "via": "auto"}),
        )
        .await;
        return;
    };

    // Bot harus admin untuk mengeksekusi apa pun.
    if !is_chat_privileged(state, &token, bot_id, chat_id, bot_tg_id).await {
        record_moderation(
            state,
            bot_id,
            chat_id,
            Some(user_id),
            "logged_only_not_admin",
            Some("Pelanggaran terdeteksi tetapi bot bukan admin di grup ini"),
            serde_json::json!({"chat_id": chat_id, "user_id": user_id, "message_id": msg.message_id, "violations": violation_names, "via": "auto"}),
        )
        .await;
        return;
    }

    // Admin/creator grup tidak pernah dibatasi.
    if is_chat_privileged(state, &token, bot_id, chat_id, user_id).await {
        return;
    }

    // Eksekusi: selalu hapus pesan yang melanggar.
    let delete_ok = state
        .telegram
        .delete_message(&token, chat_id, msg.message_id)
        .await
        .is_ok();
    if !delete_ok {
        tracing::warn!("Gagal menghapus pesan melanggar di chat {chat_id}");
    }

    // Aksi tambahan: flood selalu mute; selain itu ikut violation_action
    // (mute/warn). violation_action=delete berarti hanya hapus.
    let mut action_taken = "auto_delete";
    if flooded {
        let until =
            chrono::Utc::now().timestamp() + gs.mute_duration_minutes.max(0) * 60;
        if state
            .telegram
            .restrict_chat_member_until(&token, chat_id, user_id, false, Some(until))
            .await
            .is_ok()
        {
            action_taken = "auto_mute_flood";
        }
    } else if gs.violation_action == "mute" {
        let until =
            chrono::Utc::now().timestamp() + gs.mute_duration_minutes.max(0) * 60;
        if state
            .telegram
            .restrict_chat_member_until(&token, chat_id, user_id, false, Some(until))
            .await
            .is_ok()
        {
            action_taken = "auto_mute";
        }
    } else if gs.violation_action == "warn" {
        let first_name = msg
            .from
            .as_ref()
            .map(|u| u.first_name.clone())
            .unwrap_or_default();
        let warn_text = format!(
            "⚠️ {first_name}, pesanmu dihapus karena melanggar aturan grup."
        );
        let _ = state
            .telegram
            .send_message(&token, &SendMessageParams {
                chat_id,
                text: warn_text,
                ..Default::default()
            })
            .await;
        action_taken = "auto_warn";
    }

    record_moderation(
        state,
        bot_id,
        chat_id,
        Some(user_id),
        action_taken,
        Some(&format!("Pelanggaran otomatis: {}", violation_names.join(", "))),
        serde_json::json!({
            "chat_id": chat_id,
            "user_id": user_id,
            "message_id": msg.message_id,
            "violations": violation_names,
            "action": action_taken,
            "delete_ok": delete_ok,
            "via": "auto",
        }),
    )
    .await;
}

/// Moderasi grup dasar (legacy, dari `bot_chats.settings` JSON):
/// keyword filter, link filter, flood detection.
/// Eksekusi (hapus pesan) HANYA bila bot admin; bila bukan admin, hanya catat log.
/// Dipakai bila tidak ada aturan v1.3 yang aktif di `group_settings`.
async fn moderate_group_message_legacy(
    state: &AppState,
    bot_id: &str,
    msg: &TelegramMessage,
    text: &str,
) {
    let chat_id = msg.chat.id;
    let user_id = match msg.from.as_ref() {
        Some(u) => u.id,
        None => return,
    };
    let chat_row = sqlx::query_as::<_, BotChat>(
        "SELECT * FROM bot_chats WHERE bot_id = ? AND chat_id = ?",
    )
    .bind(bot_id)
    .bind(chat_id)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let Some(chat_row) = chat_row else { return };
    let settings: serde_json::Value =
        serde_json::from_str(&chat_row.settings).unwrap_or(serde_json::json!({}));
    let is_admin = chat_row.is_bot_admin == 1;

    let mut violations: Vec<String> = Vec::new();

    // Keyword filter.
    if settings
        .get("keyword_filter")
        .and_then(|k| k.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let keywords: Vec<String> = settings
            .get("keyword_filter")
            .and_then(|k| k.get("keywords"))
            .and_then(|k| k.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();
        let lower = text.to_lowercase();
        if keywords.iter().any(|kw| !kw.is_empty() && lower.contains(kw)) {
            violations.push("keyword_filter".to_string());
        }
    }

    // Link filter.
    if settings
        .get("link_filter")
        .and_then(|k| k.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let lower = text.to_lowercase();
        if lower.contains("http://")
            || lower.contains("https://")
            || lower.contains("www.")
            || lower.contains("t.me/")
        {
            violations.push("link_filter".to_string());
        }
    }

    // Flood / anti-spam: hitung pesan user dalam 60 detik terakhir.
    let flood_max = settings
        .get("anti_spam")
        .and_then(|a| a.get("max_per_minute"))
        .and_then(|v| v.as_u64())
        .or_else(|| {
            settings
                .get("flood")
                .and_then(|a| a.get("max_per_minute"))
                .and_then(|v| v.as_u64())
        })
        .unwrap_or(0) as usize;
    let anti_spam_on = settings
        .get("anti_spam")
        .and_then(|a| a.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || settings
            .get("flood")
            .and_then(|a| a.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
    if anti_spam_on && flood_max > 0 {
        let count = state.flood.record(bot_id, chat_id, user_id);
        if count > flood_max {
            violations.push("flood".to_string());
        }
    }

    if violations.is_empty() {
        return;
    }

    let action_taken = if is_admin {
        // Bot admin: hapus pesan yang melanggar.
        match get_bot_token(state, bot_id).await {
            Ok(token) => match state
                .telegram
                .delete_message(&token, chat_id, msg.message_id)
                .await
            {
                Ok(_) => "message_deleted",
                Err(e) => {
                    tracing::warn!("Gagal menghapus pesan melanggar di chat {chat_id}: {e}");
                    "delete_failed"
                }
            },
            Err(_) => "delete_failed",
        }
    } else {
        // Bukan admin: HANYA catat, jangan eksekusi moderasi.
        "logged_only_not_admin"
    };

    log_activity(
        state,
        Some(bot_id),
        "moderation",
        Some("moderation"),
        Some(&chat_id.to_string()),
        serde_json::json!({
            "chat_id": chat_id,
            "user_id": user_id,
            "message_id": msg.message_id,
            "violations": violations,
            "action": action_taken,
        }),
        "warning",
    )
    .await;
}

/// Command kustom: cari command aktif, naikkan usage_count, kirim
/// response_template sebagai auto-reply.
async fn handle_command(state: &AppState, bot_id: &str, msg: &TelegramMessage, text: &str) {
    let first_word = text.split_whitespace().next().unwrap_or("");
    let mut name = first_word.trim_start_matches('/').to_string();
    // Potong suffix @botusername (mis. /start@mybot).
    if let Some(at) = name.find('@') {
        name.truncate(at);
    }
    if name.is_empty() {
        return;
    }
    let command = sqlx::query_as::<_, BotCommand>(
        "SELECT * FROM bot_commands WHERE bot_id = ? AND name = ? AND is_enabled = 1",
    )
    .bind(bot_id)
    .bind(&name)
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    let Some(command) = command else { return };

    let _ = sqlx::query(
        "UPDATE bot_commands SET usage_count = usage_count + 1, updated_at = ? WHERE id = ?",
    )
    .bind(now_rfc3339())
    .bind(&command.id)
    .execute(&state.pool)
    .await;

    if command.response_template.trim().is_empty() {
        return;
    }
    let context = serde_json::json!({
        "first_name": msg.from.as_ref().map(|u| u.first_name.clone()),
        "username": msg.from.as_ref().and_then(|u| u.username.clone()),
        "chat_id": msg.chat.id,
        "text": text,
    });
    let reply = render_template(&command.response_template, &context);
    if let Ok(token) = get_bot_token(state, bot_id).await {
        let _ = state
            .telegram
            .send_message(&token, &SendMessageParams {
                chat_id: msg.chat.id,
                text: reply,
                reply_to_message_id: Some(msg.message_id),
                ..Default::default()
            })
            .await;
    }
    log_activity(
        state,
        Some(bot_id),
        "command_executed",
        Some("bot_command"),
        Some(&command.id),
        serde_json::json!({"command": name, "chat_id": msg.chat.id}),
        "info",
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::telegram::Update;

    /// AppState test: SQLite in-memory + migrasi, token palsu terenkripsi.
    /// Panggilan ke api.telegram.org dengan token palsu SELALU gagal
    /// (401 / tidak terjangkau) — penegakan harus mencatat ke log dan
    /// melanjutkan, bukan crash. Itulah yang diuji di sini.
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
        let crypto =
            std::sync::Arc::new(crate::security::Crypto::from_key_bytes([9u8; 32]));
        let encrypted = crypto.encrypt("123456:FAKE-TOKEN").unwrap();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO bots (id, name, token_encrypted, token_masked, status, created_at, updated_at) \
             VALUES ('bot-1', 'Bot Test', ?, '****', 'disconnected', ?, ?)",
        )
        .bind(&encrypted)
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        let telegram = crate::bot::telegram::TelegramClient::new().unwrap();
        AppState::new(pool, config, crypto, telegram)
    }

    async fn seed_group(state: &AppState, chat_id: i64, settings: &str) {
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO bot_chats (id, bot_id, chat_id, type, title, created_at, updated_at) \
             VALUES (?, 'bot-1', ?, 'supergroup', 'Grup Test', ?, ?)",
        )
        .bind(format!("chat-{chat_id}"))
        .bind(chat_id)
        .bind(&now)
        .bind(&now)
        .execute(&state.pool)
        .await
        .unwrap();
        // settings = daftar nilai kolom SETELAH chat_id, sesuai urutan:
        // welcome_enabled, welcome_message, anti_spam_enabled, flood_limit,
        // flood_window_seconds, block_links, banned_words, violation_action,
        // mute_duration_minutes.
        sqlx::query(&format!(
            "INSERT INTO group_settings (chat_id, welcome_enabled, welcome_message, \
             anti_spam_enabled, flood_limit, flood_window_seconds, block_links, \
             banned_words, violation_action, mute_duration_minutes) \
             VALUES (?, {settings})"
        ))
        .bind(chat_id)
        .execute(&state.pool)
        .await
        .unwrap();
    }

    fn group_update(message_id: i64, chat_id: i64, user_id: i64, text: &str) -> Update {
        serde_json::from_value(serde_json::json!({
            "update_id": message_id,
            "message": {
                "message_id": message_id,
                "from": {"id": user_id, "is_bot": false, "first_name": "Budi"},
                "chat": {"id": chat_id, "type": "supergroup", "title": "Grup Test"},
                "date": 1793412000,
                "text": text,
            }
        }))
        .unwrap()
    }

    async fn mod_log_count(state: &AppState, chat_id: i64) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM group_moderation_log WHERE chat_id = ?",
        )
        .bind(chat_id)
        .fetch_one(&state.pool)
        .await
        .unwrap()
    }

    async fn mod_log_actions(state: &AppState, chat_id: i64) -> Vec<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT action FROM group_moderation_log WHERE chat_id = ? ORDER BY created_at",
        )
        .bind(chat_id)
        .fetch_all(&state.pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn log_activity_redacts_token_in_details() {
        let state = test_state().await;
        let token = format!("123456789:{}", "FAKE".repeat(9));
        log_activity(
            &state,
            Some("bot-1"),
            "watchdog_failure",
            Some("bot"),
            Some("bot-1"),
            serde_json::json!({"error": format!("https://api.telegram.org/bot{token}/getMe gagal")}),
            "error",
        )
        .await;
        let details: String = sqlx::query_scalar(
            "SELECT details FROM activity_logs WHERE action = 'watchdog_failure' LIMIT 1",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert!(!details.contains(&token));
        assert!(details.contains("****"));
    }

    #[tokio::test]
    async fn banned_word_message_is_logged_not_crashed() {
        let state = test_state().await;
        seed_group(&state, -100123, "0, NULL, 1, 5, 10, 1, '[\"judi\"]', 'delete', 60").await;
        process_update(&state, "bot-1", &group_update(1, -100123, 777, "ayo main JUDI bareng")).await;
        assert_eq!(mod_log_count(&state, -100123).await, 1);
        let actions = mod_log_actions(&state, -100123).await;
        assert!(
            actions[0].starts_with("logged_only"),
            "dengan token palsu penegakan dicatat, bukan dieksekusi: {}",
            actions[0]
        );
        // Pesan tetap tersimpan normal (tidak hilang karena moderasi).
        let saved: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE bot_id = 'bot-1' AND chat_id = -100123",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert_eq!(saved, 1);
        // activity_logs juga mendapat baris moderation.
        let acts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_logs WHERE action = 'moderation'",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert_eq!(acts, 1);
    }

    #[tokio::test]
    async fn link_message_is_logged_and_clean_message_is_not() {
        let state = test_state().await;
        seed_group(&state, -100124, "0, NULL, 0, 5, 10, 1, '[]', 'delete', 60").await;
        process_update(&state, "bot-1", &group_update(1, -100124, 777, "halo semua, apa kabar?")).await;
        assert_eq!(mod_log_count(&state, -100124).await, 0);
        process_update(&state, "bot-1", &group_update(2, -100124, 777, "cek https://example.com yuk")).await;
        assert_eq!(mod_log_count(&state, -100124).await, 1);
    }

    #[tokio::test]
    async fn flood_triggers_after_limit_within_window() {
        let state = test_state().await;
        seed_group(&state, -100125, "0, NULL, 1, 2, 60, 0, '[]', 'delete', 30").await;
        process_update(&state, "bot-1", &group_update(1, -100125, 777, "pesan satu")).await;
        process_update(&state, "bot-1", &group_update(2, -100125, 777, "pesan dua")).await;
        assert_eq!(mod_log_count(&state, -100125).await, 0);
        process_update(&state, "bot-1", &group_update(3, -100125, 777, "pesan tiga")).await;
        assert_eq!(mod_log_count(&state, -100125).await, 1);
    }

    #[tokio::test]
    async fn welcome_event_does_not_crash_and_is_logged() {
        let state = test_state().await;
        seed_group(
            &state,
            -100126,
            "1, 'Halo {{first_name}} di {{chat_title}}!', 0, 5, 10, 0, '[]', 'delete', 60",
        )
        .await;
        // Substitusi template sudah diuji unit di template_vars; di sini
        // pastikan event join tidak crash dan tercatat user_joined_group
        // (pengiriman welcome dengan token palsu gagal diam-diam).
        let update: Update = serde_json::from_value(serde_json::json!({
            "update_id": 1,
            "message": {
                "message_id": 1,
                "from": {"id": 111, "is_bot": false, "first_name": "Admin"},
                "chat": {"id": -100126, "type": "supergroup", "title": "Grup Test"},
                "date": 1793412000,
                "new_chat_members": [{"id": 333, "is_bot": false, "first_name": "Citra"}]
            }
        }))
        .unwrap();
        process_update(&state, "bot-1", &update).await;
        let joined: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity_logs WHERE action = 'user_joined_group'",
        )
        .fetch_one(&state.pool)
        .await
        .unwrap();
        assert_eq!(joined, 1);
    }
}
