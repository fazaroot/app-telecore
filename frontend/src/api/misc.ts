import { apiDownload, apiRequest, asArray } from './client'
import type {
  AnalyticsSummary,
  AppSettings,
  Backup,
  NotificationItem,
  PerBotStat,
  PersonalToken,
  SecurityStatus,
} from '../types'

type RawRecord = Record<string, any>

export async function fetchAnalyticsSummary(from?: string, to?: string): Promise<AnalyticsSummary> {
  const raw = (await apiRequest<unknown>('/api/analytics/summary', {
    query: { from, to },
  })) as RawRecord | null
  if (!raw || typeof raw !== 'object') return {}
  // Petakan bentuk backend {totals, messages_per_day, command_usage, ...}
  // ke bentuk yang dipakai Dashboard/Analytics.
  const daily = ((raw.messages_per_day ?? raw.daily ?? []) as RawRecord[]).map((d) => ({
    date: (d.day ?? d.date ?? '') as string,
    messages_in: (d.inbound ?? d.messages_in ?? 0) as number,
    messages_out: (d.outbound ?? d.messages_out ?? 0) as number,
    users: (d.users ?? 0) as number,
  }))
  const commandUsage = ((raw.command_usage ?? []) as RawRecord[]).map((c) => ({
    command: (c.command ?? c.name ?? '—') as string,
    count: (c.count ?? c.usage_count ?? 0) as number,
  }))
  return {
    ...raw,
    messages_in: raw.totals?.inbound ?? raw.messages_in,
    messages_out: raw.totals?.outbound ?? raw.messages_out,
    total_users: raw.total_users,
    failed_requests: raw.totals?.failed ?? raw.failed_requests,
    avg_response_ms: raw.avg_response_time_ms ?? raw.avg_response_ms,
    daily,
    command_usage: commandUsage,
  }
}

export async function exportAnalytics(format: 'csv' | 'json', from?: string, to?: string): Promise<void> {
  await apiDownload('/api/analytics/export', `telecore-analytics.${format}`, { format, from, to })
}

export async function fetchNotifications(): Promise<NotificationItem[]> {
  const data = await apiRequest<unknown>('/api/notifications')
  return asArray<NotificationItem>(data, ['notifications', 'items', 'data'])
}

export async function markNotificationRead(id: string | number): Promise<void> {
  await apiRequest(`/api/notifications/${id}/read`, { method: 'POST' })
}

export async function markAllNotificationsRead(): Promise<void> {
  await apiRequest('/api/notifications/read-all', { method: 'POST' })
}

export async function deleteNotification(id: string | number): Promise<void> {
  await apiRequest(`/api/notifications/${id}`, { method: 'DELETE' })
}

export async function fetchSecurityStatus(): Promise<SecurityStatus> {
  const raw = (await apiRequest<unknown>('/api/security/status')) as RawRecord | null
  if (!raw || typeof raw !== 'object') return {}
  return {
    ...raw,
    encryption_enabled: raw.encryption_active ?? raw.encryption_enabled,
    token_encryption: raw.encryption_algorithm ?? raw.token_encryption,
    pin_set: raw.pin_configured ?? raw.pin_set,
  }
}

export async function changePin(payload: { current_pin?: string; new_pin: string }): Promise<unknown> {
  return apiRequest('/api/security/change-pin', { method: 'POST', body: payload })
}

export async function fetchSessions(): Promise<SecurityStatus['sessions']> {
  const data = await apiRequest<unknown>('/api/security/sessions')
  return asArray(data, ['sessions', 'items', 'data'])
}

export async function fetchBackups(): Promise<Backup[]> {
  const data = await apiRequest<unknown>('/api/backups')
  return asArray<Backup>(data, ['backups', 'items', 'data'])
}

export async function createBackup(passphrase: string): Promise<Backup> {
  return apiRequest<Backup>('/api/backups', { method: 'POST', body: { passphrase } })
}

export async function restoreBackup(id: string | number, passphrase: string): Promise<unknown> {
  return apiRequest(`/api/backups/${id}/restore`, { method: 'POST', body: { passphrase } })
}

export async function deleteBackup(id: string | number): Promise<void> {
  await apiRequest(`/api/backups/${id}`, { method: 'DELETE' })
}

export async function fetchAppSettings(): Promise<AppSettings> {
  return apiRequest<AppSettings>('/api/settings')
}

export async function updateAppSettings(settings: Partial<AppSettings>): Promise<AppSettings> {
  return apiRequest<AppSettings>('/api/settings', { method: 'PUT', body: settings })
}

// --- v1.1: Personal API tokens ---

export async function fetchPersonalTokens(): Promise<PersonalToken[]> {
  const data = await apiRequest<unknown>('/api/security/tokens')
  return asArray<PersonalToken>(data, ['tokens', 'items', 'data'])
}

export async function createPersonalToken(name: string): Promise<PersonalToken> {
  return apiRequest<PersonalToken>('/api/security/tokens', { method: 'POST', body: { name } })
}

export async function revokePersonalToken(id: string | number): Promise<void> {
  await apiRequest(`/api/security/tokens/${id}`, { method: 'DELETE' })
}

// --- v1.1: Per-bot analytics ---

export async function fetchPerBotAnalytics(from?: string, to?: string): Promise<PerBotStat[]> {
  const data = await apiRequest<unknown>('/api/analytics/per-bot', { query: { from, to } })
  return asArray<PerBotStat>(data, ['bots', 'per_bot', 'items', 'data'])
}

export async function exportPerBotAnalyticsCsv(from?: string, to?: string): Promise<void> {
  await apiDownload('/api/analytics/per-bot/export', 'telecore-analytics-per-bot.csv', {
    from,
    to,
  })
}
