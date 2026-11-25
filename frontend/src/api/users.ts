import { apiDownload, apiRequest, asArray } from './client'
import type { Group, GroupSettings, ModerationLogEntry, TelegramUser } from '../types'

export interface UserFilters {
  bot_id?: string | number
  search?: string
  blocked?: boolean
  label?: string
  tag_id?: string | number
  segment_id?: string | number
}

export async function fetchUsers(filters: UserFilters = {}): Promise<TelegramUser[]> {
  // Kontrak backend (ListUsersQuery): nama parameternya `q` dan
  // `is_blocked`, bukan `search`/`blocked` — param yang salah nama akan
  // diabaikan diam-diam oleh serde, jadi petakan di sini.
  const data = await apiRequest<unknown>('/api/users', {
    query: {
      bot_id: filters.bot_id,
      q: filters.search,
      is_blocked: filters.blocked === undefined ? undefined : String(filters.blocked),
      label: filters.label,
      tag_id: filters.tag_id,
      segment_id: filters.segment_id,
    },
  })
  return asArray<TelegramUser>(data, ['users', 'items', 'data'])
}

export async function fetchUser(id: string | number): Promise<TelegramUser> {
  return apiRequest<TelegramUser>(`/api/users/${id}`)
}

export async function updateUser(
  id: string | number,
  payload: { notes?: string; labels?: string[] },
): Promise<TelegramUser> {
  return apiRequest<TelegramUser>(`/api/users/${id}`, { method: 'PUT', body: payload })
}

export async function blockUser(id: string | number): Promise<TelegramUser> {
  return apiRequest<TelegramUser>(`/api/users/${id}/block`, { method: 'POST' })
}

export async function unblockUser(id: string | number): Promise<TelegramUser> {
  return apiRequest<TelegramUser>(`/api/users/${id}/unblock`, { method: 'POST' })
}

export async function exportUsersCsv(botId?: string | number): Promise<void> {
  await apiDownload('/api/users/export', 'telecore-users.csv', {
    format: 'csv',
    bot_id: botId,
  })
}

export async function fetchGroups(botId?: string | number): Promise<Group[]> {
  const data = await apiRequest<unknown>('/api/groups', {
    query: botId !== undefined ? { bot_id: botId } : undefined,
  })
  return asArray<Group>(data, ['groups', 'items', 'data'])
}

export async function fetchGroup(id: string | number): Promise<Group> {
  return apiRequest<Group>(`/api/groups/${id}`)
}

/** Kontrak v1.3: PUT /api/groups/{id}/settings (partial, body langsung settings). */
export async function updateGroupSettings(id: string | number, settings: GroupSettings): Promise<GroupSettings> {
  return apiRequest<GroupSettings>(`/api/groups/${id}/settings`, { method: 'PUT', body: settings })
}

export async function fetchModerationLogs(groupId: string | number): Promise<ModerationLogEntry[]> {
  // Kontrak v1.3: GET /api/groups/{id}/moderation-log (bisa paginasi {items, total, ...}).
  const data = await apiRequest<unknown>(`/api/groups/${groupId}/moderation-log`)
  return asArray<ModerationLogEntry>(data, ['logs', 'items', 'data'])
}
