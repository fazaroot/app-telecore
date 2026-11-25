-- TeleCore initial schema (SQLite).
-- Semua timestamp disimpan sebagai string RFC3339 (UTC).
-- Semua boolean disimpan sebagai INTEGER 0/1.

CREATE TABLE IF NOT EXISTS bots (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    username TEXT,
    telegram_id INTEGER,
    token_encrypted TEXT NOT NULL,
    token_masked TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'disconnected',
    is_active INTEGER NOT NULL DEFAULT 1,
    description TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_bots_status ON bots(status);

CREATE TABLE IF NOT EXISTS bot_configurations (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL UNIQUE REFERENCES bots(id) ON DELETE CASCADE,
    webhook_url TEXT,
    polling_enabled INTEGER NOT NULL DEFAULT 1,
    webhook_enabled INTEGER NOT NULL DEFAULT 0,
    webhook_secret TEXT,
    last_update_offset INTEGER NOT NULL DEFAULT 0,
    auto_reply_enabled INTEGER NOT NULL DEFAULT 0,
    welcome_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_bot_configurations_bot_id ON bot_configurations(bot_id);

CREATE TABLE IF NOT EXISTS bot_commands (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    response_template TEXT NOT NULL DEFAULT '',
    aliases TEXT NOT NULL DEFAULT '[]',
    permissions TEXT NOT NULL DEFAULT '[]',
    is_enabled INTEGER NOT NULL DEFAULT 1,
    usage_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(bot_id, name)
);
CREATE INDEX IF NOT EXISTS idx_bot_commands_bot_id ON bot_commands(bot_id);

CREATE TABLE IF NOT EXISTS bot_users (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    telegram_user_id INTEGER NOT NULL,
    username TEXT,
    first_name TEXT,
    last_name TEXT,
    language_code TEXT,
    is_blocked INTEGER NOT NULL DEFAULT 0,
    is_bot INTEGER NOT NULL DEFAULT 0,
    notes TEXT,
    labels TEXT NOT NULL DEFAULT '[]',
    last_seen_at TEXT,
    message_count INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(bot_id, telegram_user_id)
);
CREATE INDEX IF NOT EXISTS idx_bot_users_bot_id ON bot_users(bot_id);
CREATE INDEX IF NOT EXISTS idx_bot_users_telegram_user_id ON bot_users(telegram_user_id);
CREATE INDEX IF NOT EXISTS idx_bot_users_created_at ON bot_users(created_at);

CREATE TABLE IF NOT EXISTS bot_chats (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    chat_id INTEGER NOT NULL,
    type TEXT NOT NULL DEFAULT 'private',
    title TEXT,
    username TEXT,
    is_bot_admin INTEGER NOT NULL DEFAULT 0,
    settings TEXT NOT NULL DEFAULT '{}',
    member_count INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(bot_id, chat_id)
);
CREATE INDEX IF NOT EXISTS idx_bot_chats_bot_id ON bot_chats(bot_id);
CREATE INDEX IF NOT EXISTS idx_bot_chats_chat_id ON bot_chats(chat_id);
CREATE INDEX IF NOT EXISTS idx_bot_chats_type ON bot_chats(type);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    chat_id INTEGER NOT NULL,
    telegram_message_id INTEGER,
    direction TEXT NOT NULL DEFAULT 'inbound',
    message_type TEXT NOT NULL DEFAULT 'text',
    content TEXT,
    media_file_id TEXT,
    status TEXT NOT NULL DEFAULT 'received',
    from_user_id INTEGER,
    reply_to_message_id INTEGER,
    parse_mode TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_bot_id_created_at ON messages(bot_id, created_at);
CREATE INDEX IF NOT EXISTS idx_messages_chat_id ON messages(chat_id);
CREATE INDEX IF NOT EXISTS idx_messages_direction ON messages(direction);
CREATE INDEX IF NOT EXISTS idx_messages_status ON messages(status);

CREATE TABLE IF NOT EXISTS message_templates (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    content TEXT NOT NULL,
    category TEXT,
    variables TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_message_templates_bot_id ON message_templates(bot_id);

CREATE TABLE IF NOT EXISTS automation_workflows (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    trigger_type TEXT NOT NULL,
    trigger_config TEXT NOT NULL DEFAULT '{}',
    conditions TEXT NOT NULL DEFAULT '[]',
    actions TEXT NOT NULL DEFAULT '[]',
    is_enabled INTEGER NOT NULL DEFAULT 1,
    execution_count INTEGER NOT NULL DEFAULT 0,
    last_executed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_automation_workflows_bot_id ON automation_workflows(bot_id);
CREATE INDEX IF NOT EXISTS idx_automation_workflows_trigger ON automation_workflows(trigger_type);

CREATE TABLE IF NOT EXISTS automation_executions (
    id TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES automation_workflows(id) ON DELETE CASCADE,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    trigger_event TEXT,
    status TEXT NOT NULL DEFAULT 'success',
    steps_executed INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_automation_executions_workflow_id ON automation_executions(workflow_id);
CREATE INDEX IF NOT EXISTS idx_automation_executions_created_at ON automation_executions(created_at);

CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id TEXT PRIMARY KEY,
    bot_id TEXT NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    task_type TEXT NOT NULL,
    chat_id INTEGER NOT NULL,
    message_content TEXT NOT NULL,
    media_url TEXT,
    cron_expression TEXT,
    interval_seconds INTEGER,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    next_run_at TEXT,
    last_run_at TEXT,
    run_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    is_paused INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_next_run_at ON scheduled_tasks(next_run_at);
CREATE INDEX IF NOT EXISTS idx_scheduled_tasks_bot_id ON scheduled_tasks(bot_id);

CREATE TABLE IF NOT EXISTS notifications (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    data TEXT NOT NULL DEFAULT '{}',
    is_read INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_notifications_is_read ON notifications(is_read);
CREATE INDEX IF NOT EXISTS idx_notifications_created_at ON notifications(created_at);

CREATE TABLE IF NOT EXISTS activity_logs (
    id TEXT PRIMARY KEY,
    bot_id TEXT REFERENCES bots(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    entity_type TEXT,
    entity_id TEXT,
    details TEXT NOT NULL DEFAULT '{}',
    level TEXT NOT NULL DEFAULT 'info',
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_activity_logs_bot_id_created_at ON activity_logs(bot_id, created_at);
CREATE INDEX IF NOT EXISTS idx_activity_logs_entity ON activity_logs(entity_type, entity_id);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS backups (
    id TEXT PRIMARY KEY,
    filename TEXT NOT NULL,
    size_bytes INTEGER NOT NULL DEFAULT 0,
    description TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_backups_created_at ON backups(created_at);

CREATE TABLE IF NOT EXISTS admin_sessions (
    id TEXT PRIMARY KEY,
    pin_hash TEXT,
    salt TEXT,
    is_locked INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_unlocked_at TEXT
);
