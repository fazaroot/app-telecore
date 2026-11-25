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
