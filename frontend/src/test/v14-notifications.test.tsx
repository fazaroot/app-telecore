import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { redactBotTokens } from '../utils/redact'

/**
 * Regresi v1.4: pesan notifikasi berisi URL panjang keluar menyamping
 * (overflow) dan token bot tampil utuh di pesan watchdog. UI harus
 * me-wrap teks panjang tanpa spasi dan meredaksi pola token bot.
 * Token di test ini sintetis dan jelas palsu.
 */
const FAKE_TOKEN = '123456789:' + 'A'.repeat(35)
const LONG_URL = `https://example.com/${'a'.repeat(140)}`

vi.mock('../api/misc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/misc')>()
  return {
    ...actual,
    fetchNotifications: vi.fn(async () => [
      {
        id: 'n1',
        title: 'Watchdog gagal menghubungi bot',
        message: `Gagal menghubungi https://api.telegram.org/bot${FAKE_TOKEN}/getMe lalu ${LONG_URL}`,
        type: 'watchdog_failure',
        severity: 'error',
        is_read: false,
        created_at: '2026-11-25T10:00:00Z',
      },
    ]),
  }
})

import { NotificationsPage } from '../pages/NotificationsPage'

describe('redactBotTokens (v1.4)', () => {
  it('menyembunyikan pola token bot sintetis', () => {
    const out = redactBotTokens(`url https://api.telegram.org/bot${FAKE_TOKEN}/getMe`)
    expect(out).not.toContain(FAKE_TOKEN)
    expect(out).toContain('[token disembunyikan]')
  })

  it('tidak mengubah teks biasa', () => {
    expect(redactBotTokens('Bot terputus, coba lagi nanti.')).toBe('Bot terputus, coba lagi nanti.')
  })
})

describe('regresi v1.4: notifikasi tidak overflow dan token tidak tampil', () => {
  it('paragraf pesan memakai class wrap dan token sintetis tidak tampil utuh', async () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
    })
    render(
      <QueryClientProvider client={queryClient}>
        <NotificationsPage />
      </QueryClientProvider>,
    )

    const message = await screen.findByText(
      (content) => content.includes('Gagal menghubungi'),
      {},
      { timeout: 10_000 },
    )
    expect(message.className).toContain('break-words')
    expect(message.className).toContain('[overflow-wrap:anywhere]')
    expect(message.className).toContain('min-w-0')

    const title = screen.getByText('Watchdog gagal menghubungi bot')
    expect(title.className).toContain('break-words')
    expect(title.className).toContain('[overflow-wrap:anywhere]')

    expect(document.body.textContent).not.toContain(FAKE_TOKEN)
    expect(document.body.textContent).toContain('[token disembunyikan]')
  }, 20_000)
})
