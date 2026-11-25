import { useEffect, useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Pause, Play, ScrollText } from 'lucide-react'
import { fetchLogs, mergeLogEntries, normalizeLogEntry } from '../api/logs'
import { useWebSocket } from '../hooks/useWebSocket'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import type { LogEntry, WsEvent } from '../types'

const MAX_LINES = 200

function levelTone(level?: string): 'default' | 'success' | 'danger' | 'warning' | 'info' {
  const l = (level ?? '').toLowerCase()
  if (l === 'error' || l === 'fatal') return 'danger'
  if (l === 'warn' || l === 'warning') return 'warning'
  if (l === 'info') return 'info'
  if (l === 'debug' || l === 'trace') return 'default'
  return 'default'
}

function logText(e: LogEntry): string {
  return String(e.message ?? (typeof e === 'string' ? e : JSON.stringify(e)))
}

function logFromWsEvent(ev: WsEvent): LogEntry | null {
  if (ev.type !== 'log') return null
  const p = ev.payload
  if (p && typeof p === 'object') return normalizeLogEntry(p)
  if (typeof p === 'string') return normalizeLogEntry(p)
  return null
}

export function LogsPage() {
  const [entries, setEntries] = useState<LogEntry[]>([])
  const [autoScroll, setAutoScroll] = useState(true)
  const [loaded, setLoaded] = useState(false)
  const boxRef = useRef<HTMLDivElement>(null)

  const { status: wsStatus } = useWebSocket((ev) => {
    const entry = logFromWsEvent(ev)
    if (entry) setEntries((cur) => mergeLogEntries(cur, [entry], MAX_LINES))
  })

  // Muat awal dari server; bila WS tidak terhubung, polling tiap 5 detik.
  const initialQuery = useQuery({
    queryKey: ['logs'],
    queryFn: () => fetchLogs(100),
    refetchInterval: wsStatus === 'open' ? false : 5000,
  })

  useEffect(() => {
    if (initialQuery.data) {
      setEntries((cur) => {
        const merged = mergeLogEntries(cur, initialQuery.data ?? [], MAX_LINES)
        // Urutkan kronologis bila ada timestamp.
        return [...merged].sort((a, b) =>
          String(a.timestamp ?? a.created_at ?? '').localeCompare(String(b.timestamp ?? b.created_at ?? '')),
        )
      })
      setLoaded(true)
    }
  }, [initialQuery.data])

  // Auto-scroll ke bawah saat ada baris baru (kecuali dijeda pengguna).
  useEffect(() => {
    if (!autoScroll) return
    const el = boxRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [entries, autoScroll])

  return (
    <div className="space-y-4">
      <PageHeader
        title="Logs"
        subtitle="Log server secara live"
        action={
          <button
            type="button"
            onClick={() => setAutoScroll((s) => !s)}
            className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-2 text-sm font-semibold dark:bg-darktext/10"
          >
            {autoScroll ? <Pause className="h-4 w-4" /> : <Play className="h-4 w-4" />}
            {autoScroll ? 'Jeda auto-scroll' : 'Lanjutkan auto-scroll'}
          </button>
        }
      />

      <Card>
        <div className="mb-3 flex flex-wrap items-center gap-2 text-sm">
          <Badge tone={wsStatus === 'open' ? 'success' : 'warning'}>
            {wsStatus === 'open' ? 'Live (WebSocket)' : wsStatus === 'connecting' ? 'Menghubungkan…' : 'Polling 5 detik (WS terputus)'}
          </Badge>
          <span className="text-ink/50 dark:text-darktext/50">
            {entries.length} baris (maks. {MAX_LINES} terakhir)
          </span>
        </div>
        {initialQuery.isLoading && !loaded ? (
          <LoadingSpinner label="Memuat log…" />
        ) : initialQuery.isError && !loaded ? (
          <ErrorState message={(initialQuery.error as Error).message} onRetry={() => void initialQuery.refetch()} />
        ) : entries.length === 0 ? (
          <EmptyState
            icon={<ScrollText className="h-10 w-10 text-primary" />}
            title="Belum ada log"
            description="Log server akan muncul di sini saat ada aktivitas."
          />
        ) : (
          <div
            ref={boxRef}
            className="max-h-[60vh] overflow-y-auto rounded-xl bg-ink/5 p-3 font-mono text-xs leading-relaxed dark:bg-black/30"
          >
            {entries.map((e, i) => (
              <div key={String(e.id ?? `${e.timestamp ?? ''}-${i}`)} className="flex gap-2 py-0.5">
                <span className="shrink-0 text-ink/40 dark:text-darktext/40">
                  {(e.timestamp ?? e.created_at ?? '').slice(11, 19) ||
                    (e.timestamp ?? e.created_at ?? '')}
                </span>
                {e.level && <Badge tone={levelTone(e.level)}>{e.level.toUpperCase()}</Badge>}
                <span className="break-words">{logText(e)}</span>
              </div>
            ))}
          </div>
        )}
      </Card>
    </div>
  )
}
