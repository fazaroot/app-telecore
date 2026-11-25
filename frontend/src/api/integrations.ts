import { apiRequest, asArray } from './client'
import { WEBHOOK_EVENTS, type OutgoingWebhook, type WebhookDelivery } from '../types'

export const OUTGOING_WEBHOOK_EVENTS: readonly string[] = WEBHOOK_EVENTS

export async function fetchOutgoingWebhooks(): Promise<OutgoingWebhook[]> {
  const data = await apiRequest<unknown>('/api/outgoing-webhooks')
  return asArray<OutgoingWebhook>(data, ['webhooks', 'items', 'data'])
}

export interface OutgoingWebhookPayload {
  name: string
  url: string
  events: string[]
  is_enabled?: boolean
}

export async function createOutgoingWebhook(payload: OutgoingWebhookPayload): Promise<OutgoingWebhook> {
  return apiRequest<OutgoingWebhook>('/api/outgoing-webhooks', { method: 'POST', body: payload })
}

export async function updateOutgoingWebhook(
  id: string | number,
  payload: Partial<OutgoingWebhookPayload>,
): Promise<OutgoingWebhook> {
  return apiRequest<OutgoingWebhook>(`/api/outgoing-webhooks/${id}`, { method: 'PUT', body: payload })
}

export async function deleteOutgoingWebhook(id: string | number): Promise<void> {
  await apiRequest(`/api/outgoing-webhooks/${id}`, { method: 'DELETE' })
}

export async function fetchWebhookDeliveries(id: string | number): Promise<WebhookDelivery[]> {
  const data = await apiRequest<unknown>(`/api/outgoing-webhooks/${id}/deliveries`)
  return asArray<WebhookDelivery>(data, ['deliveries', 'items', 'data'])
}

export async function testOutgoingWebhook(id: string | number): Promise<unknown> {
  return apiRequest(`/api/outgoing-webhooks/${id}/test`, { method: 'POST' })
}

export async function retryWebhookDelivery(
  webhookId: string | number,
  deliveryId: string | number,
): Promise<unknown> {
  return apiRequest(`/api/outgoing-webhooks/${webhookId}/deliveries/${deliveryId}/retry`, {
    method: 'POST',
  })
}

export interface ClaudeDesktopConfig {
  mcpServers: {
    telecore: {
      url: string
      headers: { 'X-API-Key': string }
    }
  }
}

/** Konfigurasi JSON untuk Claude Desktop (MCP server TeleCore). */
export function buildClaudeDesktopConfig(mcpUrl: string, apiKey: string): ClaudeDesktopConfig {
  return {
    mcpServers: {
      telecore: {
        url: mcpUrl,
        headers: { 'X-API-Key': apiKey },
      },
    },
  }
}

export function claudeDesktopConfigJson(mcpUrl: string, apiKey: string): string {
  return JSON.stringify(buildClaudeDesktopConfig(mcpUrl, apiKey), null, 2)
}

/** Tes koneksi MCP dengan handshake JSON-RPC 2.0 `initialize` (POST /mcp). */
export async function testMcpConnection(mcpPath = '/mcp'): Promise<unknown> {
  return apiRequest<unknown>(mcpPath, {
    method: 'POST',
    body: {
      jsonrpc: '2.0',
      id: 'telecore-test',
      method: 'initialize',
      params: {
        protocolVersion: '2024-11-05',
        capabilities: {},
        clientInfo: { name: 'TeleCore', version: '1.3.0' },
      },
    },
  })
}
