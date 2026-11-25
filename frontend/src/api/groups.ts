import { apiRequest, asArray } from './client'
import type { GroupSettings, GroupStats, ModerationLogEntry } from '../types'

export type ViolationAction = 'delete' | 'warn' | 'mute'
export type ModerationAction = 'mute' | 'unmute' | 'ban' | 'unban'

export async function fetchGroupSettings(id: string | number): Promise<GroupSettings> {
  return apiRequest<GroupSettings>(`/api/groups/${id}/settings`)
}

export async function updateGroupSettings(
  id: string | number,
  settings: Partial<GroupSettings>,
): Promise<GroupSettings> {
  return apiRequest<GroupSettings>(`/api/groups/${id}/settings`, { method: 'PUT', body: settings })
}

export async function moderateGroupMember(
  groupId: string | number,
  payload: { user_id: string | number; action: ModerationAction; duration_minutes?: number; reason?: string },
): Promise<unknown> {
  // Kontrak backend: ModerateRequest.user_id adalah i64 — string "123"
  // ditolak serde dengan 422. Koersi di sini (satu choke point) agar
  // input teks dari GroupsPage selalu terkirim sebagai angka.
  const userId = typeof payload.user_id === 'string' ? Number(payload.user_id.trim()) : payload.user_id
  if (typeof userId !== 'number' || !Number.isFinite(userId)) {
    throw new Error('ID pengguna harus berupa angka Telegram user ID.')
  }
  return apiRequest(`/api/groups/${groupId}/moderate`, {
    method: 'POST',
    body: { ...payload, user_id: userId },
  })
}

export async function fetchGroupStats(id: string | number): Promise<GroupStats> {
  return apiRequest<GroupStats>(`/api/groups/${id}/stats`)
}

export async function fetchModerationLog(groupId: string | number): Promise<ModerationLogEntry[]> {
  const data = await apiRequest<unknown>(`/api/groups/${groupId}/moderation-log`)
  return asArray<ModerationLogEntry>(data, ['logs', 'items', 'data'])
}
