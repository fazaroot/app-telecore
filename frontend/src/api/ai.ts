import { apiRequest, asArray } from './client'
import type { AiProvider, AiProviderType, BotAiSettings } from '../types'

// --- Preset provider sekali-klik (v1.2) ---

export interface AiProviderPreset {
  label: string
  provider_type: AiProviderType
  base_url: string
  model: string
}

export const AI_PROVIDER_PRESETS: Record<string, AiProviderPreset> = {
  openai: {
    label: 'OpenAI',
    provider_type: 'openai_compatible',
    base_url: 'https://api.openai.com/v1',
    model: 'gpt-4o-mini',
  },
  anthropic: {
    label: 'Anthropic',
    provider_type: 'anthropic',
    base_url: 'https://api.anthropic.com',
    model: 'claude-3-5-sonnet-20241022',
  },
  gemini: {
    label: 'Gemini',
    provider_type: 'gemini',
    base_url: 'https://generativelanguage.googleapis.com/v1beta',
    model: 'gemini-2.0-flash',
  },
  deepseek: {
    label: 'DeepSeek',
    provider_type: 'openai_compatible',
    base_url: 'https://api.deepseek.com/v1',
    model: 'deepseek-chat',
  },
  groq: {
    label: 'Groq',
    provider_type: 'openai_compatible',
    base_url: 'https://api.groq.com/openai/v1',
    model: 'llama-3.3-70b-versatile',
  },
  openrouter: {
    label: 'OpenRouter',
    provider_type: 'openai_compatible',
    base_url: 'https://openrouter.ai/api/v1',
    model: 'openai/gpt-4o-mini',
  },
}

/** Terapkan preset: kembalikan base_url + model default untuk tipe provider. */
export function applyAiPreset(key: string): { provider_type: AiProviderType; base_url: string; model: string } | null {
  const p = AI_PROVIDER_PRESETS[key]
  if (!p) return null
  return { provider_type: p.provider_type, base_url: p.base_url, model: p.model }
}

export const AI_PROVIDER_TYPE_LABELS: Record<string, string> = {
  openai_compatible: 'OpenAI-compatible',
  anthropic: 'Anthropic',
  gemini: 'Gemini',
}

// --- CRUD provider ---

export async function fetchAiProviders(): Promise<AiProvider[]> {
  const data = await apiRequest<unknown>('/api/ai-providers')
  return asArray<AiProvider>(data, ['providers', 'items', 'data'])
}

export interface AiProviderPayload {
  name: string
  provider_type: AiProviderType | string
  base_url: string
  model: string
  api_key?: string
  enabled?: boolean
}

export async function createAiProvider(payload: AiProviderPayload): Promise<AiProvider> {
  return apiRequest<AiProvider>('/api/ai-providers', { method: 'POST', body: payload })
}

export async function updateAiProvider(
  id: string | number,
  payload: Partial<AiProviderPayload>,
): Promise<AiProvider> {
  return apiRequest<AiProvider>(`/api/ai-providers/${id}`, { method: 'PUT', body: payload })
}

export async function deleteAiProvider(id: string | number): Promise<void> {
  await apiRequest(`/api/ai-providers/${id}`, { method: 'DELETE' })
}

export async function testAiProvider(
  id: string | number,
): Promise<{ ok?: boolean; message?: string; models_count?: number }> {
  return apiRequest(`/api/ai-providers/${id}/test`, { method: 'POST' })
}

// --- AI per bot ---

export async function updateBotAi(id: string | number, settings: BotAiSettings): Promise<unknown> {
  return apiRequest(`/api/bots/${id}/ai`, { method: 'PUT', body: settings })
}

export function maskApiKey(last4?: string | null): string {
  return last4 ? `••••${last4}` : '•••• (tersimpan terenkripsi)'
}
