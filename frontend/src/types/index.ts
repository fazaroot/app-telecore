// Tipe API TeleCore. Field dibuat toleran (opsional) karena backend bisa
// mengembalikan variasi kecil — UI tidak boleh crash karena field hilang.

export interface ApiEnvelope<T> {
  success: boolean
  data?: T
  error?: { code?: string; message?: string }
}

export interface Health {
  status?: string
  db?: string
  database?: string
  uptime?: number | string
  uptime_seconds?: number
  version?: string
}

export type ConnectionStatus = 'connected' | 'disconnected' | 'error' | 'never_tested'

export interface Bot {
  id: string | number
  name?: string
  username?: string
  telegram_id?: number | string
  token_masked?: string
  is_active?: boolean
  connection_status?: ConnectionStatus | string
  polling_enabled?: boolean
  webhook_url?: string | null
  webhook_enabled?: boolean
  created_at?: string
  updated_at?: string
}

export interface BotConfig {
  polling_enabled?: boolean
  webhook_url?: string | null
  webhook_enabled?: boolean
  [key: string]: unknown
}

export interface Message {
  id: string | number
  bot_id?: string | number
  chat_id?: string | number
  direction?: 'in' | 'out' | 'inbound' | 'outbound' | string
  message_type?: string
  text?: string
  content?: string
  status?: 'pending' | 'sent' | 'delivered' | 'failed' | string
  telegram_message_id?: number | string
  created_at?: string
  error?: string | null
  error_message?: string | null
}

export interface Command {
  id: string | number
  bot_id?: string | number
  command?: string
  name?: string
  description?: string
  response?: string
  response_template?: string
  aliases?: string[]
  permissions?: string[]
  usage_count?: number
  is_enabled?: boolean
  created_at?: string
  updated_at?: string
}

export interface TelegramUser {
  id: string | number
  bot_id?: string | number
  telegram_id?: number | string
  telegram_user_id?: number | string
  username?: string | null
  first_name?: string
  last_name?: string | null
  notes?: string | null
  labels?: string[]
  is_blocked?: boolean
  last_seen?: string
  last_seen_at?: string
  created_at?: string
}

export interface Group {
  id: string | number
  bot_id?: string | number
  telegram_id?: number | string
  chat_id?: number | string
  title?: string
  name?: string
  member_count?: number
  bot_is_admin?: boolean
  is_bot_admin?: boolean
  settings?: GroupSettings
  created_at?: string
}

export interface GroupSettings {
  welcome_message?: string
  keyword_filter?: string[]
  anti_spam?: boolean
  link_filter?: boolean
  flood_detection?: boolean
  warnings_enabled?: boolean
  max_warnings?: number
  [key: string]: unknown
}

export interface ModerationLogEntry {
  id: string | number
  group_id?: string | number
  user_id?: string | number
  action?: string
  reason?: string
  created_at?: string
}

export interface Workflow {
  id: string | number
  bot_id?: string | number
  name?: string
  trigger_type?: string
  trigger_config?: Record<string, unknown>
  conditions?: WorkflowCondition[]
  actions?: WorkflowAction[]
  is_enabled?: boolean
  created_at?: string
  updated_at?: string
}

export interface WorkflowCondition {
  field?: string
  operator?: string
  value?: string
}

export interface WorkflowAction {
  type?: string
  delay_seconds?: number
  text?: string
  [key: string]: unknown
}

export interface WorkflowExecution {
  id: string | number
  workflow_id?: string | number
  status?: string
  started_at?: string
  finished_at?: string
  created_at?: string
  error?: string | null
  error_message?: string | null
  steps_executed?: number
  duration_ms?: number
}

export interface ScheduledTask {
  id: string | number
  bot_id?: string | number
  name?: string
  schedule_type?: 'one_time' | 'daily' | 'weekly' | 'interval' | string
  task_type?: 'one-time' | 'daily' | 'weekly' | 'interval' | 'cron' | string
  cron?: string
  cron_expression?: string
  run_at?: string
  time?: string
  day_of_week?: number | string
  interval_minutes?: number
  interval_seconds?: number
  timezone?: string
  chat_id?: string | number
  message?: string
  message_content?: string
  text?: string
  is_active?: boolean
  is_paused?: boolean
  last_run_at?: string
  next_run_at?: string
  created_at?: string
}

export interface SchedulerHistoryEntry {
  id: string | number
  task_id?: string | number
  status?: string
  run_at?: string
  error?: string | null
  action?: string
  level?: string
  created_at?: string
  details?: Record<string, unknown> | string
}

export interface AnalyticsSummary {
  messages_in?: number
  messages_out?: number
  total_users?: number
  active_users?: number
  new_users?: number
  engagement_rate?: number
  automation_runs?: number
  automation_success_rate?: number
  failed_requests?: number
  avg_response_ms?: number
  daily?: DailyStat[]
  command_usage?: { command?: string; count?: number }[]
  [key: string]: unknown
}

export interface DailyStat {
  date?: string
  messages_in?: number
  messages_out?: number
  users?: number
  [key: string]: unknown
}

export interface NotificationItem {
  id: string | number
  title?: string
  message?: string
  body?: string
  type?: string
  severity?: string
  is_read?: boolean
  created_at?: string
}

export interface SecurityStatus {
  token_encryption?: string
  encryption_enabled?: boolean
  pin_set?: boolean
  pin_enabled?: boolean
  biometric_available?: boolean
  sessions?: Session[]
}

export interface Session {
  id: string | number
  device?: string
  ip?: string
  last_active?: string
  created_at?: string
}

export interface Backup {
  id: string | number
  name?: string
  filename?: string
  size?: number
  size_bytes?: number
  created_at?: string
}

export interface AppSettings {
  base_url?: string
  theme?: 'light' | 'dark'
  language?: string
  auto_lock_minutes?: number
  [key: string]: unknown
}

export interface WsEvent {
  type: 'activity' | 'bot_status' | 'message' | 'notification' | 'stats' | string
  payload?: Record<string, unknown> | unknown
  ts?: string
}

export interface ActivityItem {
  id: string
  text: string
  ts: string
  kind: string
}

export interface LocalTemplate {
  id: string
  name: string
  text: string
  parse_mode: 'MarkdownV2' | 'HTML' | 'none'
  created_at: string
}
