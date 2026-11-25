import { describe, expect, it } from 'vitest'
import { buildClaudeDesktopConfig, claudeDesktopConfigJson } from '../api/integrations'
import { mergeLogEntries, normalizeLogEntry, parseLogLine } from '../api/logs'

describe('buildClaudeDesktopConfig', () => {
  it('membuat konfigurasi mcpServers telecore dengan url dan header X-API-Key', () => {
    const cfg = buildClaudeDesktopConfig('https://example.com/mcp', 'kunci-rahasia')
    expect(cfg).toEqual({
      mcpServers: {
        telecore: {
          url: 'https://example.com/mcp',
          headers: { 'X-API-Key': 'kunci-rahasia' },
        },
      },
    })
  })

  it('menghasilkan JSON yang bisa diparse kembali', () => {
    const json = claudeDesktopConfigJson('https://example.com/mcp', 'kunci-rahasia')
    expect(JSON.parse(json)).toEqual(buildClaudeDesktopConfig('https://example.com/mcp', 'kunci-rahasia'))
  })
})

describe('mergeLogEntries', () => {
  it('menambah baris baru tanpa duplikat dan memotong ke 200 terakhir', () => {
    const existing = [{ id: 1, message: 'a' }, { id: 2, message: 'b' }]
    const merged = mergeLogEntries(existing, [{ id: 2, message: 'b' }, { id: 3, message: 'c' }])
    expect(merged).toHaveLength(3)
    expect(merged[2]).toEqual({ id: 3, message: 'c' })

    const many = Array.from({ length: 250 }, (_, i) => ({ id: i, message: `m${i}` }))
    expect(mergeLogEntries([], many)).toHaveLength(200)
    expect(mergeLogEntries([], many)[0]).toEqual({ id: 50, message: 'm50' })
  })
})

describe('parseLogLine / normalizeLogEntry (format log backend v1.1)', () => {
  it('mengurai baris "timestamp [LEVEL] pesan" dari GET /api/logs', () => {
    const line = '2026-11-25T03:00:00.123Z [INFO] server jalan port=1945'
    const e = parseLogLine(line)
    expect(e.timestamp).toBe('2026-11-25T03:00:00.123Z')
    expect(e.level).toBe('info')
    expect(e.message).toBe(line)
  })

  it('mengembalikan baris tanpa format sebagai message mentah', () => {
    expect(parseLogLine('baris bebas')).toEqual({ message: 'baris bebas' })
  })

  it('menormalisasi payload WS {line, level, message}', () => {
    const e = normalizeLogEntry({
      line: '2026-11-25T03:00:00.123Z [ERROR] gagal kirim',
      level: 'ERROR',
      message: 'gagal kirim',
    })
    expect(e?.timestamp).toBe('2026-11-25T03:00:00.123Z')
    expect(e?.level).toBe('ERROR')
    expect(e?.message).toContain('gagal kirim')
  })

  it('mengembalikan null untuk entri kosong', () => {
    expect(normalizeLogEntry(null)).toBeNull()
    expect(normalizeLogEntry(undefined)).toBeNull()
  })
})
