import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { Bot } from '../types'

/**
 * Regresi bug laporan Astr (2026-11-25): halaman My Bots blank putih penuh
 * di APK v1.2.0. Penyebab: `<BotAiSettingsForm botId={configBot!.id} />`
 * dievaluasi saat BotsPage di-render (elemen JSX dibuat walau Modal tertutup),
 * sementara configBot masih null → TypeError, dan tanpa ErrorBoundary seluruh
 * aplikasi unmount jadi layar putih.
 *
 * API di-mock dengan payload persis data live Astr (1 bot "Bot testing"),
 * jadi test ini aman dijalankan di CI tanpa backend.
 */
vi.mock('../api/bots', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/bots')>()
  const liveBot: Bot = {
    id: '50806391-f7e9-4d87-a7f4-6dc06fa13d04',
    name: 'Bot testing',
    username: 'siantars_bot',
    telegram_id: 9172081566,
    token_masked: '****Gyrs',
    connection_status: 'connected',
    is_active: true,
    created_at: '2026-11-25T02:47:23.983299530+00:00',
    updated_at: '2026-11-25T04:48:41.090416761+00:00',
  }
  return { ...actual, fetchBots: vi.fn(async () => [liveBot]) }
})

vi.mock('../api/ai', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/ai')>()
  return { ...actual, fetchAiProviders: vi.fn(async () => []) }
})

import { BotsPage } from '../pages/BotsPage'

describe('regresi: My Bots tidak boleh blank putih', () => {
  it('menampilkan kartu bot saat daftar berhasil dimuat', async () => {
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
    })
    render(
      <QueryClientProvider client={queryClient}>
        <BotsPage />
      </QueryClientProvider>,
    )

    expect(await screen.findByText('Bot testing', {}, { timeout: 10_000 })).toBeInTheDocument()
    expect(screen.getByText('@siantars_bot')).toBeInTheDocument()
    expect(screen.getByText('Terhubung')).toBeInTheDocument()
  }, 20_000)
})
