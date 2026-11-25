import { apiRequest, asArray } from './client'
import type { LogEntry } from '../types'

/**
 * Satu baris log backend berbentuk teks:
 *   "2026-11-25T03:00:00.000Z [INFO] pesan log field=val"
 * Parser ini mengekstrak timestamp + level; pesan tampil utuh (baris asli).
 */
export function parseLogLine(line: string): LogEntry {
  const m = line.match(/^(\S+) \[([A-Za-z]+)\] (.*)$/s)
  if (m) return { timestamp: m[1], level: m[2].toLowerCase(), message: line }
  return { message: line }
}

/** Normalisasi satu entri log dari bentuk apa pun yang dikirim backend. */
export function normalizeLogEntry(raw: unknown): LogEntry | null {
  if (raw === null || raw === undefined) return null
  if (typeof raw === 'string') return parseLogLine(raw)
  if (typeof raw === 'object') {
    const o = raw as Record<string, unknown>
    // Payload event WS "log": {line, level, message}
    if (typeof o.line === 'string') {
      const parsed = parseLogLine(o.line)
      return {
        ...parsed,
        level: typeof o.level === 'string' ? o.level : parsed.level,
        message: o.line,
      }
    }
    if (typeof o.message === 'string' || typeof o.text === 'string') {
      return {
        id: o.id as string | number | undefined,
        timestamp: (o.timestamp ?? o.created_at ?? o.ts) as string | undefined,
        level: o.level as string | undefined,
        message: (o.message ?? o.text) as string,
      }
    }
  }
  return null
}

/** Log server terbaru (paling lama 100 baris per permintaan awal). */
export async function fetchLogs(limit = 100): Promise<LogEntry[]> {
  const data = await apiRequest<unknown>('/api/logs', { query: { limit } })
  // Backend mengembalikan {lines: string[], ...}; asArray juga menerima
  // bentuk lain bila backend berubah.
  const arr = asArray<unknown>(data, ['lines', 'logs', 'items', 'data'])
  const out: LogEntry[] = []
  for (const e of arr) {
    const n = normalizeLogEntry(e)
    if (n) out.push(n)
  }
  return out
}

/** Gabungkan baris log baru ke daftar lama (dedupe + potong 200 terakhir). */
export function mergeLogEntries(existing: LogEntry[], incoming: LogEntry[], max = 200): LogEntry[] {
  const key = (e: LogEntry) =>
    `${String(e.id ?? '')}|${String(e.timestamp ?? e.created_at ?? '')}|${String(e.message ?? '')}`
  const seen = new Set(existing.map(key))
  const merged = [...existing]
  for (const e of incoming) {
    if (!seen.has(key(e))) {
      seen.add(key(e))
      merged.push(e)
    }
  }
  return merged.slice(-max)
}
