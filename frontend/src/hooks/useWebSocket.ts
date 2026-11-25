import { useCallback, useEffect, useRef, useState } from 'react'
import { wsUrl } from '../api/client'
import { useSettingsStore } from '../stores/settings'
import type { WsEvent } from '../types'

export type WsStatus = 'connecting' | 'open' | 'closed'

/**
 * Koneksi WebSocket ke backend (/ws). Reconnect otomatis dengan backoff.
 * Bila WS putus, halaman memakai polling TanStack Query sebagai fallback.
 */
export function useWebSocket(onEvent?: (event: WsEvent) => void) {
  const [status, setStatus] = useState<WsStatus>('connecting')
  const [lastEvent, setLastEvent] = useState<WsEvent | null>(null)
  const baseUrl = useSettingsStore((s) => s.baseUrl)
  const handlerRef = useRef(onEvent)
  handlerRef.current = onEvent
  const retryRef = useRef(0)
  const wsRef = useRef<WebSocket | null>(null)

  useEffect(() => {
    let cancelled = false
    let timer: number | undefined

    const connect = () => {
      if (cancelled) return
      setStatus('connecting')
      let ws: WebSocket
      try {
        ws = new WebSocket(wsUrl())
      } catch {
        scheduleReconnect()
        return
      }
      wsRef.current = ws

      ws.onopen = () => {
        if (cancelled) return
        retryRef.current = 0
        setStatus('open')
      }
      ws.onmessage = (ev) => {
        if (cancelled) return
        try {
          const parsed = JSON.parse(ev.data as string) as WsEvent
          setLastEvent(parsed)
          handlerRef.current?.(parsed)
        } catch {
          // pesan non-JSON diabaikan
        }
      }
      ws.onclose = () => {
        if (cancelled) return
        setStatus('closed')
        scheduleReconnect()
      }
      ws.onerror = () => {
        try {
          ws.close()
        } catch {
          // abaikan
        }
      }
    }

    const scheduleReconnect = () => {
      if (cancelled) return
      const delay = Math.min(1000 * 2 ** retryRef.current, 30000)
      retryRef.current += 1
      timer = window.setTimeout(connect, delay)
    }

    connect()
    return () => {
      cancelled = true
      window.clearTimeout(timer)
      try {
        wsRef.current?.close()
      } catch {
        // abaikan
      }
    }
    // baseUrl berubah → sambung ulang ke server baru
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [baseUrl])

  const send = useCallback((data: unknown) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(data))
    }
  }, [])

  return { status, lastEvent, send }
}
