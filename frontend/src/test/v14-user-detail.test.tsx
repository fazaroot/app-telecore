import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

/**
 * Regresi v1.4: Detail Pengguna tampil "User undefined" karena frontend
 * mengharapkan TelegramUser langsung, padahal backend GET /api/users/{id}
 * mengembalikan `{ user, recent_messages, activity }`. Test ini memakai
 * fetchUser asli (mock hanya di level fetch) dengan payload berbentuk
 * respons backend sebenarnya.
 */

const backendUser = {
  id: 'u1',
  bot_id: 'b1',
  telegram_user_id: 987654321,
  username: 'sari',
  first_name: 'Sari',
  last_name: 'Wulandari',
  is_blocked: false,
  is_bot: false,
  notes: null,
  labels: [],
  message_count: 42,
  last_seen_at: '2026-11-25T10:00:00Z',
  created_at: '2026-11-20T08:30:00Z',
}

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

function stubBackend() {
  fetchMock.mockImplementation(async (url: string) => {
    const u = String(url)
    if (u.includes('/api/users/u1')) {
      return envelope({
        user: backendUser,
        recent_messages: [{ id: 'm1', content: 'Halo', created_at: '2026-11-25T09:00:00Z' }],
        activity: [{ id: 'a1', action: 'message_received', created_at: '2026-11-25T09:00:00Z' }],
      })
    }
    if (u.includes('/api/users')) {
      return envelope({ items: [backendUser], total: 1, page: 1, per_page: 20 })
    }
    throw new Error(`URL tak terduga: ${u}`)
  })
  vi.stubGlobal('fetch', fetchMock)
}

vi.mock('../api/bots', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/bots')>()
  return { ...actual, fetchBots: vi.fn(async () => [{ id: 'b1', name: 'Bot testing' }]) }
})

vi.mock('../api/tags', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/tags')>()
  return {
    ...actual,
    fetchTags: vi.fn(async () => []),
    fetchUserTags: vi.fn(async () => []),
    fetchUserNotes: vi.fn(async () => []),
    fetchUserTimeline: vi.fn(async () => [
      { kind: 'message', created_at: '2026-11-25T09:00:00Z', summary: 'Mengirim pesan: Halo' },
    ]),
  }
})

vi.mock('../api/segments', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/segments')>()
  return { ...actual, fetchSegments: vi.fn(async () => []) }
})

import { fetchUser, fetchUserDetail } from '../api/users'
import { UsersPage } from '../pages/UsersPage'

describe('fetchUser unwrap respons backend (v1.4)', () => {
  it('mengembalikan user dari {user, recent_messages, activity}', async () => {
    stubBackend()
    const user = await fetchUser('u1')
    expect(user.first_name).toBe('Sari')
    expect(user.telegram_user_id).toBe(987654321)
    expect(user.created_at).toBe('2026-11-20T08:30:00Z')
  })

  it('fetchUserDetail mempertahankan recent_messages dan activity', async () => {
    stubBackend()
    const detail = await fetchUserDetail('u1')
    expect(detail.user.id).toBe('u1')
    expect(detail.recent_messages).toHaveLength(1)
    expect(detail.activity).toHaveLength(1)
  })
})

describe('regresi v1.4: Detail Pengguna menampilkan data backend', () => {
  it('nama, Telegram ID, dan tanggal bergabung tampil; tidak ada "User undefined"', async () => {
    stubBackend()
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
    })
    render(
      <QueryClientProvider client={queryClient}>
        <UsersPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByText('Sari Wulandari', {}, { timeout: 10_000 })).toBeInTheDocument()
    fireEvent.click(screen.getByText('Sari Wulandari'))

    expect(await screen.findByText(/Telegram ID: 987654321/, {}, { timeout: 10_000 })).toBeInTheDocument()
    expect(screen.getByText(/Bergabung:/)).toBeInTheDocument()
    expect(screen.getByText(/Bergabung:/).textContent).toContain('2026')
    expect(screen.getByText(/Terakhir terlihat:/)).toBeInTheDocument()
    expect(screen.getByText(/Jumlah pesan: 42/)).toBeInTheDocument()
    // "Tidak diblokir" muncul di opsi filter dan badge status detail.
    expect(screen.getAllByText('Tidak diblokir').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByText('Timeline')).toBeInTheDocument()
    expect(document.body.textContent).not.toContain('User undefined')
    expect(document.body.textContent).not.toContain('undefined')
  }, 20_000)
})
