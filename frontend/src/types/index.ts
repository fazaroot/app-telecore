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
  // v1.2: konfigurasi AI per bot (ada di BotConfiguration, bukan di Bot)
  ai_provider_id?: string | number | null
  ai_enabled?: boolean
  ai_allow_groups?: boolean
  ai_system_prompt?: string
  ai_max_tokens?: number
  ai_temperature?: number
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
  first_name?: string | null
  last_name?: string | null
  notes?: string | null
  labels?: string[]
  tags?: UserTag[]
  message_count?: number
  total_orders?: number
  has_ordered?: boolean
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
  // Kontrak v1.3 (PUT /api/groups/{id}/settings, partial)
  welcome_enabled?: boolean
  welcome_message?: string
  anti_spam_enabled?: boolean
  flood_limit?: number
  flood_window_seconds?: number
  block_links?: boolean
  banned_words?: string[]
  violation_action?: 'delete' | 'warn' | 'mute' | string
  mute_duration_minutes?: number
  // Field lama (pra-v1.3) — dipertahankan agar data lama tidak crash di UI
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

// --- v1.3: CRM — Tag, Catatan, Segmen, Timeline ---

export interface UserTag {
  id: string | number
  name: string
  color?: string
  created_at?: string
}

export interface UserNote {
  id: string | number
  user_id?: string | number
  note: string
  created_at?: string
}

export interface SegmentCriteria {
  any_tags?: (string | number)[]
  has_ordered?: boolean
  last_active_within_days?: number
  last_active_over_days_ago?: number
  min_messages?: number
}

export interface Segment {
  id: string | number
  name: string
  criteria?: SegmentCriteria
  member_count?: number
  created_at?: string
  updated_at?: string
}

export interface SegmentMembersPage {
  items: TelegramUser[]
  total: number
  page: number
  per_page: number
}

export interface SegmentFollowUpResult {
  total: number
  sent: number
  failed: number
  broadcast_id?: string | number
  /** Semua broadcast yang dibuat (satu per bot). */
  broadcast_ids?: (string | number)[]
  /** Jumlah anggota yang cocok sebelum batas 1000. */
  matched?: number
  /** true bila anggota dipotong ke 1000 penerima pertama. */
  capped_at_1000?: boolean
  /** 'running' | 'done' — status akhir setelah polling (bila ada). */
  status?: string
}

export type TimelineKind = 'message' | 'order' | 'note' | 'tag'

export interface TimelineItem {
  kind: TimelineKind | string
  created_at: string
  summary: string
  data?: Record<string, unknown>
}

// --- v1.3: Manajemen grup — Statistik ---

export interface GroupTopSender {
  user_id: string | number
  username?: string | null
  first_name?: string
  count: number
}

export interface GroupStats {
  member_count?: number
  messages_today?: number
  messages_7d?: number
  active_users_7d?: number
  top_senders?: GroupTopSender[]
}

export interface Workflow {
  id: string | number
  bot_id?: string | number
  name?: string
  trigger_type?: string
  trigger_config?: Record<string, unknown>
  conditions?: WorkflowConditions
  actions?: WorkflowAction[]
  is_enabled?: boolean
  created_at?: string
  updated_at?: string
}

export interface WorkflowCondition {
  field?: string
  operator?: string
  value?: string | number
  [key: string]: unknown
}

/**
 * Kondisi backend: array rules langsung (dianggap AND) atau objek
 * `{ logic: 'AND' | 'OR', rules: [...] }`.
 */
export type WorkflowConditions =
  | WorkflowCondition[]
  | { logic?: string; rules?: WorkflowCondition[] }

export interface WorkflowAction {
  type?: string
  text?: string
  label?: string
  note?: string
  template_id?: string
  /** Aksi `delay`: durasi tunda dalam detik (backend membatasi maks 30). */
  seconds?: number
  /** Data lama dari builder v1.3: delay per aksi (dipertahankan apa adanya). */
  delay_seconds?: number
  /** Aksi `notify_admin`: chat ID admin tujuan (opsional). */
  admin_chat_id?: number | string
  chat_id?: number | string
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
  is_automatic?: boolean | number
}

// --- v1.2: Inbox ---

export interface InboxConversation {
  chat_id: string | number
  title?: string
  username?: string | null
  first_name?: string
  chat_type?: string
  last_message_text?: string
  last_message_at?: string
  unread_count?: number
}

export interface InboxMessage {
  id?: string | number
  chat_id?: string | number
  direction?: 'in' | 'out' | 'inbound' | 'outbound' | string
  text?: string
  content?: string
  message_type?: string
  created_at?: string
  telegram_message_id?: number | string
}

export interface InboxThread {
  chat?: InboxConversation
  messages?: InboxMessage[]
}

// --- v1.2: Bot Watchdog ---

export interface WatchdogBotStatus {
  bot_id?: string | number
  username?: string
  status?: string
  last_check_at?: string
  consecutive_failures?: number
  last_error?: string | null
}

export interface WatchdogStatus {
  last_check_at?: string
  interval_minutes?: number
  auto_restart?: boolean
  bots?: WatchdogBotStatus[]
}

export interface WatchdogSettings {
  interval_minutes: number
  auto_restart: boolean
}

// --- v1.2: AI Providers ---

export type AiProviderType = 'openai_compatible' | 'anthropic' | 'gemini'

export interface AiProvider {
  id: string | number
  name?: string
  provider_type?: AiProviderType | string
  base_url?: string
  model?: string
  api_key_last4?: string | null
  enabled?: boolean
  created_at?: string
}

export interface BotAiSettings {
  ai_provider_id?: string | number | null
  ai_enabled?: boolean
  ai_allow_groups?: boolean
  ai_system_prompt?: string
  ai_max_tokens?: number
  ai_temperature?: number
}

// --- v1.2: Telegram Stars & Payments ---

export interface PaymentProduct {
  id: string | number
  bot_id?: string | number
  title?: string
  name?: string
  description?: string | null
  price?: number
  amount?: number
  currency?: string
  payload?: string
  created_at?: string
}

export interface PaymentOrder {
  id: string | number
  product_id?: string | number
  chat_id?: string | number
  amount?: number
  price?: number
  currency?: string
  status?: 'pending' | 'paid' | 'failed' | 'cancelled' | string
  created_at?: string
}

// --- v1.2: Channels ---

export interface Channel {
  chat_id: string | number
  title?: string
  bot_id?: string | number
  bot_username?: string
}

// --- v1.2: Auto-backup ---

export interface AutoBackupSettings {
  enabled?: boolean
  frequency?: 'daily' | 'weekly' | string
  hour?: number
  retention?: number
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

// --- v1.1: Integrations / MCP / Outgoing Webhooks ---

export const WEBHOOK_EVENTS = [
  'message.inbound',
  'automation.executed',
  'bot.disconnected',
  'scheduled_task.completed',
] as const

export type WebhookEvent = (typeof WEBHOOK_EVENTS)[number]

export interface OutgoingWebhook {
  id: string | number
  name?: string
  url?: string
  events?: string[]
  is_enabled?: boolean
  enabled?: boolean
  secret?: string | null
  created_at?: string
  updated_at?: string
}

export interface WebhookDelivery {
  id: string | number
  webhook_id?: string | number
  event?: string
  status?: string
  response_status?: number | null
  status_code?: number | null
  http_status?: number | null
  attempts?: number
  error?: string | null
  last_error?: string | null
  created_at?: string
  delivered_at?: string | null
}

// --- v1.1: Personal API tokens ---

export interface PersonalToken {
  id: string | number
  name?: string
  prefix?: string
  token?: string
  created_at?: string
  last_used_at?: string | null
}

// --- v1.1: Broadcast ---

export interface BroadcastRecipientResult {
  chat_id?: string | number
  status?: string
  error?: string | null
}

export interface BroadcastStatus {
  id?: string | number
  broadcast_id?: string | number
  status?: string
  total?: number
  sent?: number
  failed?: number
  results?: BroadcastRecipientResult[]
}

// --- v1.1: Per-bot analytics ---

export interface PerBotStat {
  bot_id?: string | number
  bot_name?: string
  name?: string
  username?: string
  messages_in?: number
  messages_out?: number
  active_users?: number
  new_users?: number
  automations?: number
  automation_runs?: number
  failed?: number
  failed_requests?: number
}

// --- v1.1: Live logs ---

export interface LogEntry {
  id?: string | number
  timestamp?: string
  created_at?: string
  level?: string
  message?: string
  source?: string
  [key: string]: unknown
}
