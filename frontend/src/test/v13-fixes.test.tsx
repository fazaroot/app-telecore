/**
 * Regresi perbaikan verifikasi/integrasi v1.3 (bukan klaim subagen paralel):
 * 1. Follow-up segmen TIDAK BOLEH menampilkan snapshot awal backend
 *    (sent 0, status running) sebagai hasil akhir — harus polling broadcast
 *    sampai 'done' lalu menampilkan angka final yang diagregasi.
 * 2. Moderasi grup harus mengirim user_id sebagai angka (backend i64) —
 *    string "123" ditolak serde dengan 422.
 * 3. GET /api/users: frontend harus memakai nama param backend `q` dan
 *    `is_blocked`, bukan `search`/`blocked` (diabaikan diam-diam).
 */
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { fetchUsers } from '../api/users'
import { moderateGroupMember } from '../api/groups'
import { sendSegmentFollowUpAndWait } from '../api/segments'

function envelope(data: unknown) {
  return {
    ok: true,
    status: 200,
    text: async () => JSON.stringify({ success: true, data }),
  }
}

const fetchMock = vi.fn()
afterEach(() => {
  fetchMock.mockReset()
  vi.unstubAllGlobals()
})

describe('sendSegmentFollowUpAndWait (v1.3)', () => {
  it('polling broadcast sampai done dan mengagregasi sent/failed semua bot', async () => {
    const calls: Record<string, number> = {}
    fetchMock.mockImplementation(async (url: string) => {
      const u = String(url)
      if (u.includes('/api/segments/s1/follow-up')) {
        return envelope({
          total: 2,
          matched: 2,
          capped_at_1000: false,
          sent: 0,
          failed: 0,
          broadcast_id: 'b1',
          broadcast_ids: ['b1', 'b2'],
          status: 'running',
        })
      }
      if (u.includes('/api/messages/broadcast/b1')) {
        calls.b1 = (calls.b1 ?? 0) + 1
        return envelope(
          calls.b1 === 1
            ? { broadcast_id: 'b1', status: 'running', total: 1, sent: 0, failed: 0, results: [] }
            : { broadcast_id: 'b1', status: 'done', total: 1, sent: 1, failed: 0, results: [] },
        )
      }
      if (u.includes('/api/messages/broadcast/b2')) {
        return envelope({ broadcast_id: 'b2', status: 'done', total: 1, sent: 0, failed: 1, results: [] })
      }
      throw new Error(`URL tak terduga: ${u}`)
    })
    vi.stubGlobal('fetch', fetchMock)

    const r = await sendSegmentFollowUpAndWait(
      's1',
      { message: 'Halo pelanggan!' },
      { pollIntervalMs: 1, maxAttempts: 10 },
    )
    expect(r.status).toBe('done')
    expect(r.sent).toBe(1)
    expect(r.failed).toBe(1)
    expect(r.total).toBe(2)
    // Broadcast b1 sempat 'running' dulu — polling benar-benar terjadi.
    expect(calls.b1).toBeGreaterThanOrEqual(2)
  })

  it('langsung kembali tanpa polling bila tidak ada anggota (status done)', async () => {
    fetchMock.mockImplementation(async () => envelope({
      total: 0,
      sent: 0,
      failed: 0,
      broadcast_id: null,
      broadcast_ids: [],
      status: 'done',
    }))
    vi.stubGlobal('fetch', fetchMock)

    const r = await sendSegmentFollowUpAndWait('s1', { message: 'Halo' }, { pollIntervalMs: 1 })
    expect(r.status).toBe('done')
    expect(r.total).toBe(0)
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })
})

describe('moderateGroupMember (v1.3)', () => {
  it('mengirim user_id sebagai angka, bukan string', async () => {
    let body = ''
    fetchMock.mockImplementation(async (_url: string, opts: { body?: string }) => {
      body = opts.body ?? ''
      return envelope({ success: true, action: 'mute' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await moderateGroupMember('g1', { user_id: '123456789', action: 'mute', duration_minutes: 60 })
    const parsed = JSON.parse(body) as Record<string, unknown>
    expect(parsed.user_id).toBe(123456789)
    expect(typeof parsed.user_id).toBe('number')
  })

  it('menolak user_id bukan angka sebelum request dikirim', async () => {
    vi.stubGlobal('fetch', fetchMock)
    await expect(
      moderateGroupMember('g1', { user_id: 'bukan-angka', action: 'mute' }),
    ).rejects.toThrow('angka')
    expect(fetchMock).not.toHaveBeenCalled()
  })
})

describe('fetchUsers (v1.3)', () => {
  it('memetakan search->q dan blocked->is_blocked sesuai kontrak backend', async () => {
    let seenUrl = ''
    fetchMock.mockImplementation(async (url: string) => {
      seenUrl = String(url)
      return envelope({ items: [], total: 0, page: 1, per_page: 20 })
    })
    vi.stubGlobal('fetch', fetchMock)

    await fetchUsers({ search: 'budi', blocked: true, tag_id: 't1', segment_id: 's1' })
    const q = new URL(seenUrl).searchParams
    expect(q.get('q')).toBe('budi')
    expect(q.get('is_blocked')).toBe('true')
    expect(q.get('tag_id')).toBe('t1')
    expect(q.get('segment_id')).toBe('s1')
    expect(q.has('search')).toBe(false)
    expect(q.has('blocked')).toBe(false)
  })
})

describe('SegmentsPage menampilkan hasil follow-up FINAL (v1.3)', () => {
  it('menampilkan angka akhir setelah polling, bukan snapshot 0/0', async () => {
    vi.resetModules()
    vi.doMock('../api/tags', async (importOriginal) => {
      const actual = await importOriginal<typeof import('../api/tags')>()
      return { ...actual, fetchTags: vi.fn(async () => []) }
    })
    vi.doMock('../api/segments', async (importOriginal) => {
      const actual = await importOriginal<typeof import('../api/segments')>()
      return {
        ...actual,
        fetchSegments: vi.fn(async () => [
          { id: 's1', name: 'Pelanggan aktif', criteria: {}, member_count: 2 },
        ]),
        fetchSegmentMembers: vi.fn(async () => ({
          items: [{ id: 'u1', telegram_user_id: 111, first_name: 'Budi' }],
          total: 1,
          page: 1,
          per_page: 50,
        })),
        sendSegmentFollowUpAndWait: vi.fn(async () => ({
          total: 2,
          sent: 2,
          failed: 0,
          broadcast_id: 'b1',
          broadcast_ids: ['b1'],
          status: 'done',
        })),
      }
    })
    const { SegmentsPage } = await import('../pages/SegmentsPage')
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
    })
    render(
      <QueryClientProvider client={queryClient}>
        <SegmentsPage />
      </QueryClientProvider>,
    )
    expect(await screen.findByText('Pelanggan aktif', {}, { timeout: 10_000 })).toBeInTheDocument()
    fireEvent.click(screen.getByText('Pelanggan aktif'))
    expect(await screen.findByText('Follow-up ke Segmen', {}, { timeout: 10_000 })).toBeInTheDocument()
    fireEvent.change(screen.getByPlaceholderText(/Pesan untuk semua anggota/), {
      target: { value: 'Promo hari ini!' },
    })
    fireEvent.click(screen.getByText('Kirim ke Semua Anggota'))
    expect(await screen.findByText(/Terkirim: 2 · Gagal: 0 · Total: 2/, {}, { timeout: 10_000 })).toBeInTheDocument()
  }, 20_000)
})
