import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { Group, Segment, TelegramUser, UserTag } from '../types'
import { criteriaSummary, emptyCriteria } from '../api/segments'

/**
 * Regresi pelajaran v1.2.1: modal/dialog baru v1.3 (detail user, detail grup,
 * form segmen, anggota segmen) TIDAK boleh dereferensi state nullable di props
 * JSX saat parent di-render. Test ini me-render halaman dengan data live-like
 * dan memastikan halaman tampil (tidak blank/crash) saat semua modal TERTUTUP,
 * lalu membuka tiap modal dan memastikan isinya tampil.
 */

const liveBot = {
  id: '50806391-f7e9-4d87-a7f4-6dc06fa13d04',
  name: 'Bot testing',
  username: 'siantars_bot',
  connection_status: 'connected',
  is_active: true,
}

const liveUser: TelegramUser = {
  id: 1,
  telegram_id: 123456789,
  username: 'budi',
  first_name: 'Budi',
  last_name: 'Santoso',
  tags: [{ id: 't1', name: 'VIP', color: '#16A34A' }],
  created_at: '2026-11-25T02:47:23Z',
}

const liveTags: UserTag[] = [
  { id: 't1', name: 'VIP', color: '#16A34A' },
  { id: 't2', name: 'Baru', color: '#526FE5' },
]

const liveSegment: Segment = {
  id: 's1',
  name: 'Pelanggan aktif',
  criteria: { any_tags: ['t1'], min_messages: 10 },
  member_count: 3,
}

const liveGroup: Group = {
  id: 'g1',
  title: 'Grup Toko',
  member_count: 42,
  bot_is_admin: true,
  settings: {
    welcome_enabled: true,
    welcome_message: 'Selamat datang {{first_name}} di {{chat_title}}!',
    anti_spam_enabled: true,
    flood_limit: 5,
    flood_window_seconds: 10,
    block_links: false,
    banned_words: ['spam', 'judi'],
    violation_action: 'delete',
    mute_duration_minutes: 60,
  },
}

vi.mock('../api/bots', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/bots')>()
  return { ...actual, fetchBots: vi.fn(async () => [liveBot]) }
})

vi.mock('../api/users', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/users')>()
  return {
    ...actual,
    fetchUsers: vi.fn(async () => [liveUser]),
    fetchUser: vi.fn(async () => liveUser),
    fetchGroups: vi.fn(async () => [liveGroup]),
    fetchGroup: vi.fn(async () => liveGroup),
    fetchModerationLogs: vi.fn(async () => [
      { id: 1, user_id: 123, action: 'mute', reason: 'spam', created_at: '2026-11-25T03:00:00Z' },
    ]),
    updateGroupSettings: vi.fn(async (_id: unknown, s: unknown) => s),
    updateUser: vi.fn(async () => liveUser),
  }
})

vi.mock('../api/tags', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/tags')>()
  return {
    ...actual,
    fetchTags: vi.fn(async () => liveTags),
    fetchUserTags: vi.fn(async () => liveTags.slice(0, 1)),
    fetchUserNotes: vi.fn(async () => [
      { id: 1, user_id: 1, note: 'Pelanggan setia', created_at: '2026-11-25T03:00:00Z' },
    ]),
    fetchUserTimeline: vi.fn(async () => [
      { kind: 'message', created_at: '2026-11-25T03:00:00Z', summary: 'Mengirim pesan: Halo' },
      { kind: 'order', created_at: '2026-11-25T04:00:00Z', summary: 'Order #42 lunas' },
    ]),
    createTag: vi.fn(async (p: { name: string; color?: string }) => ({ id: 't9', ...p })),
    attachUserTag: vi.fn(async () => liveTags[1]),
    detachUserTag: vi.fn(async () => undefined),
    deleteTag: vi.fn(async () => undefined),
    addUserNote: vi.fn(async () => ({ id: 9, user_id: 1, note: 'baru' })),
    deleteUserNote: vi.fn(async () => undefined),
  }
})

vi.mock('../api/segments', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/segments')>()
  return {
    ...actual,
    fetchSegments: vi.fn(async () => [liveSegment]),
    previewSegment: vi.fn(async () => ({ count: 3 })),
    fetchSegmentMembers: vi.fn(async () => ({
      items: [liveUser],
      total: 1,
      page: 1,
      per_page: 50,
    })),
    sendSegmentFollowUp: vi.fn(async () => ({ total: 1, sent: 1, failed: 0 })),
    createSegment: vi.fn(async (p: { name: string }) => ({ id: 's9', ...p })),
    updateSegment: vi.fn(async (_id: unknown, p: unknown) => ({ ...liveSegment, ...(p as object) })),
    deleteSegment: vi.fn(async () => undefined),
  }
})

vi.mock('../api/groups', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/groups')>()
  return {
    ...actual,
    fetchGroupSettings: vi.fn(async () => liveGroup.settings ?? {}),
    fetchGroupStats: vi.fn(async () => ({
      member_count: 42,
      messages_today: 120,
      messages_7d: 800,
      active_users_7d: 30,
      top_senders: [{ user_id: 123, username: 'budi', first_name: 'Budi', count: 55 }],
    })),
    moderateGroupMember: vi.fn(async () => ({ ok: true })),
    updateGroupSettings: vi.fn(async (_id: unknown, s: unknown) => s),
    fetchModerationLog: vi.fn(async () => []),
  }
})

import { UsersPage } from '../pages/UsersPage'
import { GroupsPage } from '../pages/GroupsPage'
import { SegmentsPage } from '../pages/SegmentsPage'

function renderWithClient(ui: React.ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
  })
  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

describe('regresi v1.3: UsersPage tidak crash saat modal tertutup', () => {
  it('menampilkan kartu user + badge tag, lalu detail berisi timeline & catatan', async () => {
    renderWithClient(<UsersPage />)
    expect(await screen.findByText('Budi Santoso', {}, { timeout: 10_000 })).toBeInTheDocument()
    expect(screen.getAllByText('VIP').length).toBeGreaterThan(0)
    // Modal tertutup saat render awal: konten detail belum ada.
    expect(screen.queryByText('Timeline')).not.toBeInTheDocument()

    fireEvent.click(screen.getByText('Budi Santoso'))
    expect(await screen.findByText('Timeline', {}, { timeout: 10_000 })).toBeInTheDocument()
    expect(screen.getByText('Pelanggan setia')).toBeInTheDocument()
    expect(screen.getByText('Mengirim pesan: Halo')).toBeInTheDocument()
    expect(screen.getByText('Order #42 lunas')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('Nama tag baru')).toBeInTheDocument()
  }, 20_000)
})

describe('regresi v1.3: GroupsPage tidak crash saat modal tertutup', () => {
  it('menampilkan kartu grup, lalu detail berisi pengaturan, statistik & log', async () => {
    renderWithClient(<GroupsPage />)
    expect(await screen.findByText('Grup Toko', {}, { timeout: 10_000 })).toBeInTheDocument()
    expect(screen.queryByText('Pengaturan Grup')).not.toBeInTheDocument()

    fireEvent.click(screen.getByText('Grup Toko'))
    expect(await screen.findByText('Pengaturan Grup', {}, { timeout: 10_000 })).toBeInTheDocument()
    expect(screen.getByText('Pesan hari ini')).toBeInTheDocument()
    expect(screen.getByText('120')).toBeInTheDocument()
    expect(screen.getByText('Budi (@budi)')).toBeInTheDocument()
    // 'spam' muncul dua kali: chip kata terlarang + alasan di log moderasi.
    expect(screen.getAllByText('spam').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByText('mute')).toBeInTheDocument()
    expect(screen.getByPlaceholderText(/User ID Telegram/)).toBeInTheDocument()
    expect(screen.getByText(/Template:/)).toBeInTheDocument()
  }, 20_000)
})

describe('regresi v1.3: SegmentsPage tidak crash saat modal tertutup', () => {
  it('menampilkan daftar segmen, lalu anggota + follow-up saat diklik', async () => {
    renderWithClient(<SegmentsPage />)
    expect(await screen.findByText('Pelanggan aktif', {}, { timeout: 10_000 })).toBeInTheDocument()
    expect(screen.getByText('3 anggota')).toBeInTheDocument()
    expect(screen.queryByText('Follow-up ke Segmen')).not.toBeInTheDocument()

    fireEvent.click(screen.getByText('Pelanggan aktif'))
    expect(await screen.findByText('Follow-up ke Segmen', {}, { timeout: 10_000 })).toBeInTheDocument()
    expect(await screen.findByText('Budi Santoso', {}, { timeout: 10_000 })).toBeInTheDocument()
    expect(screen.getByText('Kirim ke Semua Anggota')).toBeInTheDocument()
  }, 20_000)

  it('pembangun kriteria memanggil preview dan menampilkan jumlah anggota', async () => {
    renderWithClient(<SegmentsPage />)
    expect(await screen.findByText('Pelanggan aktif', {}, { timeout: 10_000 })).toBeInTheDocument()

    fireEvent.click(screen.getByText('+ Buat Segmen'))
    expect(await screen.findByText('Nama segmen', {}, { timeout: 10_000 })).toBeInTheDocument()
    fireEvent.click(screen.getByText('VIP'))
    expect(await screen.findByText('Preview: 3 anggota cocok dengan kriteria ini.', {}, { timeout: 10_000 })).toBeInTheDocument()
  }, 20_000)
})

describe('criteriaSummary (v1.3)', () => {
  it('merangkum semua kriteria yang terisi', () => {
    expect(
      criteriaSummary(
        { any_tags: ['t1'], has_ordered: true, last_active_within_days: 7, min_messages: 10 },
        (id) => (id === 't1' ? 'VIP' : String(id)),
      ),
    ).toBe('tag: VIP · pernah order · aktif ≤ 7 hari terakhir · min. 10 pesan')
  })

  it('mengembalikan "Semua pengguna" bila kriteria kosong', () => {
    expect(criteriaSummary({})).toBe('Semua pengguna')
    expect(criteriaSummary(undefined)).toBe('Semua pengguna')
    expect(criteriaSummary(emptyCriteria())).toBe('Semua pengguna')
  })
})
