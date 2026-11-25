import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

/**
 * v1.4: Modal Edit Bot menyediakan field "Token baru (opsional)".
 * Token hanya dikirim ke PUT /api/bots/{id} bila field diisi.
 * Token di test ini string palsu untuk test, bukan token asli.
 */
const FAKE_NEW_TOKEN = 'FAKE_TOKEN_FOR_TEST_ONLY'

vi.mock('../api/bots', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/bots')>()
  const bot = {
    id: 'b1',
    name: 'Bot testing',
    username: 'bot_testing',
    telegram_id: 123456789,
    token_masked: '****test',
    connection_status: 'connected',
    is_active: true,
  }
  return {
    ...actual,
    fetchBots: vi.fn(async () => [bot]),
    updateBot: vi.fn(async () => bot),
  }
})

vi.mock('../api/ai', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/ai')>()
  return { ...actual, fetchAiProviders: vi.fn(async () => []) }
})

import { updateBot } from '../api/bots'
import { BotsPage } from '../pages/BotsPage'

const updateBotMock = vi.mocked(updateBot)

function renderBotsPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
  })
  return render(
    <QueryClientProvider client={queryClient}>
      <BotsPage />
    </QueryClientProvider>,
  )
}

describe('ganti token bot di Modal Edit Bot (v1.4)', () => {
  it('mengirim token hanya bila field diisi', async () => {
    updateBotMock.mockClear()
    renderBotsPage()
    expect(await screen.findByText('Bot testing', {}, { timeout: 10_000 })).toBeInTheDocument()

    fireEvent.click(screen.getByText('Edit Bot'))
    const tokenInput = await screen.findByLabelText(/Token baru \(opsional\)/, {}, { timeout: 10_000 })
    expect(tokenInput).toHaveAttribute('type', 'password')
    expect(screen.getByText(/Token lama tidak ditampilkan/)).toBeInTheDocument()

    fireEvent.change(tokenInput, { target: { value: FAKE_NEW_TOKEN } })
    fireEvent.click(screen.getByText('Simpan'))

    await waitFor(() => expect(updateBotMock).toHaveBeenCalledTimes(1))
    expect(updateBotMock).toHaveBeenCalledWith('b1', { name: 'Bot testing', token: FAKE_NEW_TOKEN })
  }, 20_000)

  it('tidak mengirim token bila field kosong', async () => {
    updateBotMock.mockClear()
    renderBotsPage()
    expect(await screen.findByText('Bot testing', {}, { timeout: 10_000 })).toBeInTheDocument()

    fireEvent.click(screen.getByText('Edit Bot'))
    await screen.findByLabelText(/Token baru \(opsional\)/, {}, { timeout: 10_000 })
    fireEvent.click(screen.getByText('Simpan'))

    await waitFor(() => expect(updateBotMock).toHaveBeenCalledTimes(1))
    const payload = updateBotMock.mock.calls[0][1] as Record<string, unknown>
    expect(payload).toEqual({ name: 'Bot testing' })
    expect(payload).not.toHaveProperty('token')
  }, 20_000)
})
