// Notifikasi lokal native (Capacitor). Di web, fungsi ini diam-diam tidak
// melakukan apa-apa — semua pemanggilan dibungkus try/catch.
import { Capacitor } from '@capacitor/core'
import { LocalNotifications } from '@capacitor/local-notifications'

export async function scheduleLocalNotification(title: string, body: string): Promise<void> {
  try {
    if (!Capacitor.isNativePlatform()) return
    const perm = await LocalNotifications.checkPermissions()
    if (perm.display !== 'granted') {
      const req = await LocalNotifications.requestPermissions()
      if (req.display !== 'granted') return
    }
    await LocalNotifications.schedule({
      notifications: [
        {
          id: Math.floor(Math.random() * 1_000_000),
          title,
          body,
        },
      ],
    })
  } catch {
    // Plugin tidak tersedia / gagal — abaikan dengan aman.
  }
}

/** Tentukan apakah sebuah event WebSocket cukup penting untuk notifikasi lokal. */
export function isImportantEvent(type: string, payload: unknown): { title: string; body: string } | null {
  const p = (payload ?? {}) as Record<string, unknown>
  if (type === 'bot_status') {
    const status = String(p.status ?? p.connection_status ?? '')
    if (status === 'disconnected' || status === 'error') {
      return { title: 'Bot terputus', body: `Bot ${String(p.name ?? p.bot_id ?? '')} berstatus ${status}.` }
    }
    return null
  }
  if (type === 'notification') {
    const severity = String(p.severity ?? '')
    if (severity === 'error' || severity === 'critical') {
      return { title: String(p.title ?? 'Notifikasi penting'), body: String(p.message ?? p.body ?? '') }
    }
    return null
  }
  if (type === 'activity') {
    const kind = String(p.kind ?? '')
    if (kind === 'automation_failed' || kind === 'api_error') {
      return { title: 'Automation gagal', body: String(p.text ?? p.message ?? 'Terjadi kegagalan automation.') }
    }
  }
  return null
}
