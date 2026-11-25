-- TeleCore v1.2 schema.
-- Semua timestamp disimpan sebagai string RFC3339 (UTC).
-- Semua boolean disimpan sebagai INTEGER 0/1.
--
-- Catatan keamanan:
-- - api_key provider AI TIDAK PERNAH disimpan mentah: terenkripsi
--   AES-256-GCM dengan master key (kolom api_key_encrypted). Hanya 4
--   karakter terakhir yang disimpan terpisah (api_key_last4) untuk tampilan.
-- - provider_token pembayaran nyata TIDAK disimpan di v1.2: invoice Stars
--   memakai provider_token "" (wajib kosong untuk Stars); untuk fiat di
--   luar Stars, provider token nyata belum didukung — lihat README v1.2.

CREATE TABLE IF NOT EXISTS ai_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider_type TEXT NOT NULL,
    base_url TEXT,
    model TEXT NOT NULL,
    api_key_encrypted TEXT,
    api_key_last4 TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ai_providers_enabled ON ai_providers(enabled);

CREATE TABLE IF NOT EXISTS products (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT,
    payload TEXT NOT NULL UNIQUE,
    price INTEGER NOT NULL,
    currency TEXT NOT NULL DEFAULT 'XTR',
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_products_bot_id ON products(bot_id);

CREATE TABLE IF NOT EXISTS orders (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    product_id TEXT REFERENCES products(id) ON DELETE SET NULL,
    chat_id TEXT NOT NULL,
    invoice_payload TEXT NOT NULL,
    telegram_payment_charge_id TEXT,
    provider_payment_charge_id TEXT,
    amount INTEGER NOT NULL,
    currency TEXT NOT NULL DEFAULT 'XTR',
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    paid_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_orders_invoice_payload ON orders(invoice_payload);
CREATE INDEX IF NOT EXISTS idx_orders_bot_id ON orders(bot_id);

CREATE TABLE IF NOT EXISTS ai_usage (
    bot_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    hour_bucket TEXT NOT NULL,
    count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (bot_id, user_id, hour_bucket)
);
CREATE INDEX IF NOT EXISTS idx_ai_usage_hour_bucket ON ai_usage(hour_bucket);

ALTER TABLE bots ADD COLUMN last_check_at TEXT;
ALTER TABLE bots ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0;

ALTER TABLE bot_configurations ADD COLUMN ai_provider_id TEXT;
ALTER TABLE bot_configurations ADD COLUMN ai_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE bot_configurations ADD COLUMN ai_allow_groups INTEGER NOT NULL DEFAULT 0;
ALTER TABLE bot_configurations ADD COLUMN ai_system_prompt TEXT;
ALTER TABLE bot_configurations ADD COLUMN ai_max_tokens INTEGER NOT NULL DEFAULT 500;
ALTER TABLE bot_configurations ADD COLUMN ai_temperature REAL NOT NULL DEFAULT 0.7;

ALTER TABLE messages ADD COLUMN is_read INTEGER NOT NULL DEFAULT 1;

ALTER TABLE backups ADD COLUMN is_automatic INTEGER NOT NULL DEFAULT 0;

ALTER TABLE scheduled_tasks ADD COLUMN target_type TEXT;
ALTER TABLE scheduled_tasks ADD COLUMN target_id TEXT;

CREATE INDEX IF NOT EXISTS idx_messages_chat_is_read ON messages(chat_id, is_read);
