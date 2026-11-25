import { apiRequest, asArray } from './client'
import type { InboxConversation, InboxMessage, InboxThread } from '../types'

export async function fetchInboxConversations(): Promise<InboxConversation[]> {
  const data = await apiRequest<unknown>('/api/inbox')
  return asArray<InboxConversation>(data, ['conversations', 'items', 'data'])
}

export async function fetchInboxThread(chatId: string | number): Promise<InboxThread> {
  const data = await apiRequest<unknown>(`/api/inbox/${encodeURIComponent(String(chatId))}`)
  if (data && typeof data === 'object' && !Array.isArray(data)) {
    const raw = data as Record<string, unknown>
    const messages = asArray<InboxMessage>(raw.messages ?? raw, ['messages', 'items', 'data'])
    const chat = (raw.chat ?? {}) as InboxConversation
    return { chat, messages }
  }
  return { chat: { chat_id: '' }, messages: [] }
}

export async function replyInbox(
  chatId: string | number,
  payload: { text: string; parse_mode?: 'MarkdownV2' | 'HTML' | 'none' },
): Promise<InboxMessage> {
  return apiRequest<InboxMessage>(`/api/inbox/${encodeURIComponent(String(chatId))}/reply`, {
    method: 'POST',
    body: payload,
  })
}

export function conversationTitle(c: InboxConversation): string {
  return (
    c.title ||
    (c.first_name ? (c.username ? `${c.first_name} (@${c.username})` : c.first_name) : '') ||
    (c.username ? `@${c.username}` : '') ||
    `Chat ${c.chat_id}`
  )
}

export function conversationInitials(c: InboxConversation): string {
  const title = conversationTitle(c)
  const words = title
    .split(/\s+/)
    .map((w) => w.replace(/[^A-Za-z0-9]/g, ''))
    .filter(Boolean)
  if (words.length === 0) return '?'
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase()
  return (words[0][0] + words[1][0]).toUpperCase()
}

export function isInbound(m: InboxMessage): boolean {
  return m.direction === 'in' || m.direction === 'inbound'
}
