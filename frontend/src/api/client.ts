import { useSettingsStore, useUiStore } from '../stores/settings'
import type { ApiEnvelope } from '../types'

export class ApiError extends Error {
  code?: string
  status?: number
  constructor(message: string, code?: string, status?: number) {
    super(message)
    this.name = 'ApiError'
    this.code = code
    this.status = status
  }
}

export function getBaseUrl(): string {
  const url = useSettingsStore.getState().baseUrl.trim() || 'http://127.0.0.1:8080'
  return url.replace(/\/+$/, '')
}

function getApiKey(): string {
  return useSettingsStore.getState().apiKey
}

function buildUrl(path: string, query?: Record<string, string | number | boolean | undefined>): string {
  const url = new URL(getBaseUrl() + path)
  if (query) {
    for (const [k, v] of Object.entries(query)) {
      if (v !== undefined && v !== '' && v !== null) url.searchParams.set(k, String(v))
    }
  }
  return url.toString()
}

async function parseJsonSafe(res: Response): Promise<unknown> {
  const text = await res.text()
  if (!text) return null
  try {
    return JSON.parse(text)
  } catch {
    return text
  }
}

export async function apiRequest<T>(
  path: string,
  options: {
    method?: string
    body?: unknown
    query?: Record<string, string | number | boolean | undefined>
    skipAuth?: boolean
    signal?: AbortSignal
  } = {},
): Promise<T> {
  const { method = 'GET', body, query, skipAuth = false, signal } = options
  const headers: Record<string, string> = { Accept: 'application/json' }
  if (body !== undefined && !(body instanceof FormData)) headers['Content-Type'] = 'application/json'
  const apiKey = getApiKey()
  if (!skipAuth && apiKey) headers['X-API-Key'] = apiKey

  let res: Response
  try {
    res = await fetch(buildUrl(path, query), {
      method,
      headers,
      body: body === undefined ? undefined : body instanceof FormData ? body : JSON.stringify(body),
      signal,
    })
  } catch {
    useUiStore.getState().setBackendReachable(false)
    throw new ApiError('Backend tidak dapat dijangkau. Periksa alamat server di Settings.', 'NETWORK_ERROR')
  }

  useUiStore.getState().setBackendReachable(true)
  const parsed = await parseJsonSafe(res)

  if (!res.ok) {
    const env = parsed as ApiEnvelope<unknown> | null
    const message =
      (env && typeof env === 'object' && env.error?.message) ||
      (typeof parsed === 'string' ? parsed : null) ||
      `Permintaan gagal (HTTP ${res.status})`
    throw new ApiError(message, env && typeof env === 'object' ? env.error?.code : undefined, res.status)
  }

  const env = parsed as ApiEnvelope<T> | null
  if (env && typeof env === 'object' && 'success' in env) {
    if (!env.success) {
      throw new ApiError(env.error?.message || 'Permintaan gagal', env.error?.code, res.status)
    }
    return env.data as T
  }
  // Backend mengembalikan data mentah tanpa envelope — terima apa adanya.
  return parsed as T
}

/** Unduh file (export CSV/JSON/dll) dari endpoint API. */
export async function apiDownload(
  path: string,
  filename: string,
  query?: Record<string, string | number | boolean | undefined>,
): Promise<void> {
  const headers: Record<string, string> = {}
  const apiKey = getApiKey()
  if (apiKey) headers['X-API-Key'] = apiKey
  let res: Response
  try {
    res = await fetch(buildUrl(path, query), { headers })
  } catch {
    useUiStore.getState().setBackendReachable(false)
    throw new ApiError('Backend tidak dapat dijangkau.', 'NETWORK_ERROR')
  }
  useUiStore.getState().setBackendReachable(true)
  if (!res.ok) {
    const parsed = await parseJsonSafe(res)
    const env = parsed as ApiEnvelope<unknown> | null
    throw new ApiError(
      (env && typeof env === 'object' && env.error?.message) || `Unduhan gagal (HTTP ${res.status})`,
      undefined,
      res.status,
    )
  }
  const blob = await res.blob()
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  window.setTimeout(() => URL.revokeObjectURL(url), 5000)
}

/** Unduh teks/JSON yang dibuat di sisi klien (mis. export konfigurasi bot). */
export function downloadTextFile(filename: string, content: string, mime = 'application/json'): void {
  const blob = new Blob([content], { type: mime })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  a.remove()
  window.setTimeout(() => URL.revokeObjectURL(url), 5000)
}

export function wsUrl(): string {
  const base = getBaseUrl()
  const wsBase = base.replace(/^http:/, 'ws:').replace(/^https:/, 'wss:')
  return `${wsBase}/ws`
}

/** Ambil array dari respons yang mungkin berbentuk array langsung atau {items: []} / {bots: []}. */
export function asArray<T>(data: unknown, keys: string[] = ['items', 'data', 'results']): T[] {
  if (Array.isArray(data)) return data as T[]
  if (data && typeof data === 'object') {
    for (const key of keys) {
      const v = (data as Record<string, unknown>)[key]
      if (Array.isArray(v)) return v as T[]
    }
  }
  return []
}
