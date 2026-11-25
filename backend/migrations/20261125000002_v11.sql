-- TeleCore v1.1 schema.
-- Semua timestamp disimpan sebagai string RFC3339 (UTC).
-- Semua boolean disimpan sebagai INTEGER 0/1.
--
-- Catatan keamanan:
-- - Secret outgoing webhook TIDAK PERNAH disimpan mentah: disimpan
--   terenkripsi AES-256-GCM (kolom secret_encrypted). HMAC-SHA256 saat
--   pengiriman butuh secret mentah, jadi hash saja tidak cukup.
-- - Personal API token TIDAK PERNAH disimpan mentah: hanya SHA-256 hex
--   dari token penuh (token_hash) + prefix 8 karakter pertama.

CREATE TABLE IF NOT EXISTS outgoing_webhooks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    events TEXT NOT NULL DEFAULT '[]',
    secret_encrypted TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_outgoing_webhooks_enabled ON outgoing_webhooks(enabled);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id TEXT PRIMARY KEY,
    webhook_id TEXT NOT NULL REFERENCES outgoing_webhooks(id) ON DELETE CASCADE,
    event TEXT NOT NULL,
    payload TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'pending',
    http_status INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_webhook_id ON webhook_deliveries(webhook_id);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_created_at ON webhook_deliveries(created_at);

CREATE TABLE IF NOT EXISTS api_tokens (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    prefix TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    last_used_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_api_tokens_token_hash ON api_tokens(token_hash);
