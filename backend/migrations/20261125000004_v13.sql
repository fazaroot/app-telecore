-- TeleCore v1.3 schema (CRM + manajemen grup).
-- Semua timestamp disimpan sebagai string RFC3339 (UTC).
-- Semua boolean disimpan sebagai INTEGER 0/1.
--
-- Catatan pemetaan has_ordered (segmen):
-- - Tabel orders (v1.2) TIDAK punya kolom telegram_user_id. Satu-satunya
--   kaitan ke pengguna adalah orders.chat_id (TEXT) + orders.bot_id.
-- - Untuk invoice yang dikirim ke chat privat, chat_id == telegram_user_id
--   pengguna tersebut, sehingga has_ordered dipetakan lewat
--   CAST(orders.chat_id AS INTEGER) = bot_users.telegram_user_id
--   (dengan bot_id yang sama).
-- - Keterbatasan jujur: order yang dibuat untuk chat grup/channel (chat_id
--   negatif atau bukan ID pengguna) TIDAK bisa dipetakan ke satu pengguna
--   dan tidak dihitung sebagai "has_ordered" untuk siapa pun. Ini
--   didokumentasikan, bukan disembunyikan.

CREATE TABLE IF NOT EXISTS user_tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#526FE5',
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_user_tags_name ON user_tags(name);

CREATE TABLE IF NOT EXISTS user_tag_assignments (
    user_id TEXT NOT NULL REFERENCES bot_users(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES user_tags(id) ON DELETE CASCADE,
    assigned_at TEXT NOT NULL,
    PRIMARY KEY (user_id, tag_id)
);
CREATE INDEX IF NOT EXISTS idx_user_tag_assignments_tag_id ON user_tag_assignments(tag_id);
CREATE INDEX IF NOT EXISTS idx_user_tag_assignments_user_id ON user_tag_assignments(user_id);

CREATE TABLE IF NOT EXISTS user_notes (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES bot_users(id) ON DELETE CASCADE,
    note TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_user_notes_user_id ON user_notes(user_id);

CREATE TABLE IF NOT EXISTS segments (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    bot_id TEXT REFERENCES bots(id) ON DELETE SET NULL,
    criteria TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_segments_bot_id ON segments(bot_id);

CREATE TABLE IF NOT EXISTS group_settings (
    chat_id INTEGER PRIMARY KEY,
    welcome_enabled INTEGER NOT NULL DEFAULT 0,
    welcome_message TEXT,
    anti_spam_enabled INTEGER NOT NULL DEFAULT 0,
    flood_limit INTEGER NOT NULL DEFAULT 5,
    flood_window_seconds INTEGER NOT NULL DEFAULT 10,
    block_links INTEGER NOT NULL DEFAULT 0,
    banned_words TEXT NOT NULL DEFAULT '[]',
    violation_action TEXT NOT NULL DEFAULT 'delete',
    mute_duration_minutes INTEGER NOT NULL DEFAULT 60
);
CREATE INDEX IF NOT EXISTS idx_group_settings_chat_id ON group_settings(chat_id);

CREATE TABLE IF NOT EXISTS group_moderation_log (
    id TEXT PRIMARY KEY,
    chat_id INTEGER NOT NULL,
    user_id INTEGER,
    action TEXT NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_group_moderation_log_chat_id ON group_moderation_log(chat_id);
CREATE INDEX IF NOT EXISTS idx_group_moderation_log_created_at ON group_moderation_log(created_at);
