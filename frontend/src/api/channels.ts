import { apiRequest, asArray } from './client'
import type { Channel } from '../types'

export async function fetchChannels(): Promise<Channel[]> {
  const data = await apiRequest<unknown>('/api/channels')
  return asArray<Channel>(data, ['channels', 'items', 'data'])
}

export async function postToChannel(
  chatId: string | number,
  payload: { bot_id: string | number; text: string; parse_mode?: 'MarkdownV2' | 'HTML' | 'none' },
): Promise<unknown> {
  return apiRequest(`/api/channels/${encodeURIComponent(String(chatId))}/post`, {
    method: 'POST',
    body: payload,
  })
}

export async function scheduleChannelPost(
  chatId: string | number,
  payload: { bot_id: string | number; text: string; run_at: string; parse_mode?: 'MarkdownV2' | 'HTML' | 'none' },
): Promise<unknown> {
  return apiRequest(`/api/channels/${encodeURIComponent(String(chatId))}/schedule`, {
    method: 'POST',
    body: payload,
  })
}

/**
 * Ubah nilai input datetime-local ("2026-11-26T10:30") menjadi ISO 8601 UTC
 * untuk field run_at. Mengembalikan null bila nilainya tidak valid.
 */
export function datetimeLocalToIso(value: string): string | null {
  const trimmed = value.trim()
  if (!trimmed) return null
  const d = new Date(trimmed)
  if (Number.isNaN(d.getTime())) return null
  return d.toISOString()
}

/** Nilai minimum untuk input datetime-local (sekarang, dibulatkan ke menit). */
export function nowDatetimeLocal(): string {
  const d = new Date()
  d.setSeconds(0, 0)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}
