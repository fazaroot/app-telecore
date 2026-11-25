import { apiRequest, asArray } from './client'
import type { WatchdogBotStatus, WatchdogSettings, WatchdogStatus } from '../types'

export async function fetchWatchdogStatus(): Promise<WatchdogStatus> {
  const data = await apiRequest<unknown>('/api/watchdog/status')
  if (data && typeof data === 'object' && !Array.isArray(data)) {
    const raw = data as Record<string, unknown>
    return {
      ...raw,
      bots: asArray<WatchdogBotStatus>(raw.bots ?? raw, ['bots', 'items', 'data']),
    } as WatchdogStatus
  }
  return { bots: [] }
}

export async function updateWatchdogSettings(settings: WatchdogSettings): Promise<WatchdogSettings> {
  return apiRequest<WatchdogSettings>('/api/watchdog/settings', { method: 'PUT', body: settings })
}

export type WatchdogOverall = 'sehat' | 'gagal' | 'belum-ada-data'

/** Status keseluruhan watchdog dari daftar per bot. */
export function watchdogOverallStatus(bots: WatchdogBotStatus[]): WatchdogOverall {
  if (bots.length === 0) return 'belum-ada-data'
  const failing = bots.filter((b) => isWatchdogFailing(b))
  return failing.length > 0 ? 'gagal' : 'sehat'
}

export function isWatchdogFailing(b: WatchdogBotStatus): boolean {
  const s = (b.status ?? '').toLowerCase()
  // status di sini adalah connection_status bot: connected / disconnected /
  // error / never_tested. Bot yang terputus koneksinya dianggap bermasalah
  // oleh watchdog; never_tested hanya berarti belum pernah dicek.
  return s === 'error' || s === 'failed' || s === 'down' || s === 'disconnected' || (b.consecutive_failures ?? 0) > 0
}
