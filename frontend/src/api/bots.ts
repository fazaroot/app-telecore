import { apiDownload, apiRequest, asArray } from './client'
import type { Bot, BotConfig, Health } from '../types'

export async function fetchHealth(): Promise<Health> {
  return apiRequest<Health>('/api/health', { skipAuth: true })
}

export async function fetchBots(): Promise<Bot[]> {
  const data = await apiRequest<unknown>('/api/bots')
  return asArray<Bot>(data, ['bots', 'items', 'data'])
}

export async function fetchBot(id: string | number): Promise<Bot> {
  return apiRequest<Bot>(`/api/bots/${id}`)
}

export async function createBot(payload: { name: string; token: string }): Promise<Bot> {
  return apiRequest<Bot>('/api/bots', { method: 'POST', body: payload })
}

export async function updateBot(id: string | number, payload: Partial<Bot>): Promise<Bot> {
  return apiRequest<Bot>(`/api/bots/${id}`, { method: 'PUT', body: payload })
}

export async function deleteBot(id: string | number): Promise<void> {
  await apiRequest(`/api/bots/${id}`, { method: 'DELETE' })
}

export async function connectBot(id: string | number): Promise<Bot> {
  return apiRequest<Bot>(`/api/bots/${id}/connect`, { method: 'POST' })
}

export async function disconnectBot(id: string | number): Promise<Bot> {
  return apiRequest<Bot>(`/api/bots/${id}/disconnect`, { method: 'POST' })
}

export async function testBot(id: string | number): Promise<Record<string, unknown>> {
  return apiRequest<Record<string, unknown>>(`/api/bots/${id}/test`, { method: 'POST' })
}

export async function fetchBotConfig(id: string | number): Promise<BotConfig> {
  return apiRequest<BotConfig>(`/api/bots/${id}/config`)
}

export async function updateBotConfig(id: string | number, config: BotConfig): Promise<BotConfig> {
  return apiRequest<BotConfig>(`/api/bots/${id}/config`, { method: 'PUT', body: config })
}

export async function setBotWebhook(id: string | number, url: string): Promise<unknown> {
  return apiRequest(`/api/bots/${id}/webhook`, { method: 'POST', body: { url } })
}

export async function exportBots(): Promise<unknown> {
  return apiRequest('/api/bots/export')
}

export async function importBots(payload: unknown): Promise<unknown> {
  return apiRequest('/api/bots/import', { method: 'POST', body: payload })
}

export async function downloadBotsExport(): Promise<void> {
  await apiDownload('/api/bots/export', 'telecore-bots-export.json')
}
