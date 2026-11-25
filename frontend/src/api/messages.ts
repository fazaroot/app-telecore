import { apiRequest, asArray } from './client'
import type { Command, Message } from '../types'

export interface MessageFilters {
  bot_id?: string | number
  chat_id?: string | number
  direction?: string
  status?: string
}

export async function fetchMessages(filters: MessageFilters = {}): Promise<Message[]> {
  const data = await apiRequest<unknown>('/api/messages', { query: filters as Record<string, string | number> })
  return asArray<Message>(data, ['messages', 'items', 'data'])
}

export interface SendMessagePayload {
  bot_id: string | number
  chat_id: string | number
  message_type?: string
  text?: string
  parse_mode?: 'MarkdownV2' | 'HTML' | 'none'
  keyboard?: unknown
  media?: unknown
}

export async function sendMessage(payload: SendMessagePayload): Promise<Message> {
  return apiRequest<Message>('/api/messages', { method: 'POST', body: payload })
}

export async function retryMessage(id: string | number): Promise<Message> {
  return apiRequest<Message>(`/api/messages/${id}/retry`, { method: 'POST' })
}

export async function fetchCommands(botId?: string | number): Promise<Command[]> {
  const data = await apiRequest<unknown>('/api/commands', {
    query: botId !== undefined ? { bot_id: botId } : undefined,
  })
  return asArray<Command>(data, ['commands', 'items', 'data'])
}

export async function createCommand(payload: Partial<Command>): Promise<Command> {
  return apiRequest<Command>('/api/commands', { method: 'POST', body: payload })
}

export async function updateCommand(id: string | number, payload: Partial<Command>): Promise<Command> {
  return apiRequest<Command>(`/api/commands/${id}`, { method: 'PUT', body: payload })
}

export async function deleteCommand(id: string | number): Promise<void> {
  await apiRequest(`/api/commands/${id}`, { method: 'DELETE' })
}

export async function testCommand(id: string | number, chatId: string | number): Promise<unknown> {
  return apiRequest(`/api/commands/${id}/test`, { method: 'POST', body: { chat_id: chatId } })
}

// --- v1.1: Template variables preview ---

export interface PreviewMessagePayload {
  text: string
  first_name?: string
  username?: string
  chat_title?: string
}

export async function previewMessage(payload: PreviewMessagePayload): Promise<string> {
  const data = await apiRequest<{ rendered?: string } | string>('/api/messages/preview', {
    method: 'POST',
    body: payload,
  })
  if (typeof data === 'string') return data
  return data?.rendered ?? ''
}

// --- v1.1: Message search ---

export async function searchMessages(params: {
  q: string
  bot_id?: string | number
  limit?: number
}): Promise<Message[]> {
  const data = await apiRequest<unknown>('/api/messages/search', {
    query: { q: params.q, bot_id: params.bot_id, limit: params.limit ?? 20 },
  })
  return asArray<Message>(data, ['messages', 'results', 'items', 'data'])
}

// --- v1.1: Broadcast ---

export interface BroadcastPayload {
  bot_id: string | number
  text: string
  parse_mode?: 'MarkdownV2' | 'HTML' | 'none'
  recipients: { chat_id: string | number }[]
}

export async function sendBroadcast(
  payload: BroadcastPayload,
): Promise<{ broadcast_id?: string | number; total?: number; [key: string]: unknown }> {
  return apiRequest('/api/messages/broadcast', { method: 'POST', body: payload })
}

export async function fetchBroadcast(
  id: string | number,
): Promise<import('../types').BroadcastStatus> {
  const data = await apiRequest<unknown>(`/api/messages/broadcast/${id}`)
  if (data && typeof data === 'object' && !Array.isArray(data)) {
    return data as import('../types').BroadcastStatus
  }
  return { id, status: 'done', results: [] }
}
