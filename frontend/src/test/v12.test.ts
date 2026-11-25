import { describe, expect, it } from 'vitest'
import { AI_PROVIDER_PRESETS, applyAiPreset } from '../api/ai'
import { datetimeLocalToIso, nowDatetimeLocal } from '../api/channels'
import { watchdogOverallStatus } from '../api/watchdog'
import { messageTypeLabel, validatePollInput } from '../api/messages'
import { formatPrice } from '../api/payments'
import { conversationInitials, conversationTitle } from '../api/inbox'

describe('AI_PROVIDER_PRESETS (v1.2)', () => {
  it('preset OpenAI mengisi base_url dan model yang benar', () => {
    expect(applyAiPreset('openai')).toEqual({
      provider_type: 'openai_compatible',
      base_url: 'https://api.openai.com/v1',
      model: 'gpt-4o-mini',
    })
  })

  it('preset Anthropic mengisi base_url dan model yang benar', () => {
    expect(applyAiPreset('anthropic')).toEqual({
      provider_type: 'anthropic',
      base_url: 'https://api.anthropic.com',
      model: 'claude-3-5-sonnet-20241022',
    })
  })

  it('preset Gemini mengisi base_url dan model yang benar', () => {
    expect(applyAiPreset('gemini')).toEqual({
      provider_type: 'gemini',
      base_url: 'https://generativelanguage.googleapis.com/v1beta',
      model: 'gemini-2.0-flash',
    })
  })

  it('preset DeepSeek, Groq, OpenRouter mengisi base_url dan model yang benar', () => {
    expect(applyAiPreset('deepseek')).toEqual({
      provider_type: 'openai_compatible',
      base_url: 'https://api.deepseek.com/v1',
      model: 'deepseek-chat',
    })
    expect(applyAiPreset('groq')).toEqual({
      provider_type: 'openai_compatible',
      base_url: 'https://api.groq.com/openai/v1',
      model: 'llama-3.3-70b-versatile',
    })
    expect(applyAiPreset('openrouter')).toEqual({
      provider_type: 'openai_compatible',
      base_url: 'https://openrouter.ai/api/v1',
      model: 'openai/gpt-4o-mini',
    })
  })

  it('semua preset punya base_url https dan model tidak kosong', () => {
    for (const preset of Object.values(AI_PROVIDER_PRESETS)) {
      expect(preset.base_url.startsWith('https://')).toBe(true)
      expect(preset.model.length).toBeGreaterThan(0)
    }
  })

  it('preset tidak dikenal mengembalikan null', () => {
    expect(applyAiPreset('tidak-ada')).toBeNull()
  })
})

describe('validatePollInput (v1.2)', () => {
  const valid = {
    question: 'Kapan live?',
    options: ['Besok', 'Lusa'],
    isQuiz: false,
  }

  it('menerima poll valid dengan 2 opsi', () => {
    expect(validatePollInput(valid)).toBeNull()
  })

  it('menolak pertanyaan kosong', () => {
    expect(validatePollInput({ ...valid, question: '   ' })).toBe('Pertanyaan poll wajib diisi.')
  })

  it('menolak kurang dari 2 opsi', () => {
    expect(validatePollInput({ ...valid, options: ['Satu'] })).toContain('minimal 2 opsi')
    expect(validatePollInput({ ...valid, options: ['', '  '] })).toContain('minimal 2 opsi')
  })

  it('menolak lebih dari 10 opsi', () => {
    const options = Array.from({ length: 11 }, (_, i) => `Opsi ${i + 1}`)
    expect(validatePollInput({ ...valid, options })).toContain('maksimal 10 opsi')
  })

  it('menerima 10 opsi', () => {
    const options = Array.from({ length: 10 }, (_, i) => `Opsi ${i + 1}`)
    expect(validatePollInput({ ...valid, options })).toBeNull()
  })

  it('kuis wajib punya jawaban benar', () => {
    expect(validatePollInput({ ...valid, isQuiz: true, correctOptionId: null })).toBe(
      'Kuis membutuhkan satu jawaban benar.',
    )
  })

  it('kuis dengan jawaban benar valid', () => {
    expect(validatePollInput({ ...valid, isQuiz: true, correctOptionId: 1 })).toBeNull()
  })

  it('menolak jawaban benar di luar daftar opsi', () => {
    expect(validatePollInput({ ...valid, isQuiz: true, correctOptionId: 5 })).toBe(
      'Jawaban benar di luar daftar opsi.',
    )
  })
})

describe('datetimeLocalToIso / nowDatetimeLocal (v1.2)', () => {
  it('mengubah nilai datetime-local menjadi ISO UTC', () => {
    const iso = datetimeLocalToIso('2026-11-26T10:30')
    expect(iso).not.toBeNull()
    expect(iso).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/)
    // Sama dengan konversi Date lokal ke ISO — tidak bergantung pada zona waktu
    // mesin yang menjalankan test.
    expect(iso).toBe(new Date('2026-11-26T10:30').toISOString())
  })

  it('mengembalikan null untuk nilai kosong atau tidak valid', () => {
    expect(datetimeLocalToIso('')).toBeNull()
    expect(datetimeLocalToIso('bukan-tanggal')).toBeNull()
  })

  it('nowDatetimeLocal menghasilkan format datetime-local yang valid', () => {
    expect(nowDatetimeLocal()).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/)
    expect(datetimeLocalToIso(nowDatetimeLocal())).not.toBeNull()
  })
})

describe('watchdogOverallStatus (v1.2)', () => {
  it('belum-ada-data bila daftar bot kosong', () => {
    expect(watchdogOverallStatus([])).toBe('belum-ada-data')
  })

  it('sehat bila semua bot terhubung tanpa kegagalan', () => {
    expect(
      watchdogOverallStatus([
        { bot_id: 1, username: 'bot1', status: 'connected', consecutive_failures: 0 },
        { bot_id: 2, username: 'bot2', status: 'connected', consecutive_failures: 0 },
      ]),
    ).toBe('sehat')
  })

  it('gagal bila ada bot error, terputus, atau consecutive_failures > 0', () => {
    expect(
      watchdogOverallStatus([
        { bot_id: 1, username: 'bot1', status: 'connected', consecutive_failures: 0 },
        { bot_id: 2, username: 'bot2', status: 'error', consecutive_failures: 3, last_error: 'timeout' },
      ]),
    ).toBe('gagal')
    expect(watchdogOverallStatus([{ bot_id: 1, status: 'connected', consecutive_failures: 1 }])).toBe('gagal')
    expect(watchdogOverallStatus([{ bot_id: 1, status: 'disconnected', consecutive_failures: 0 }])).toBe('gagal')
  })
})

describe('messageTypeLabel (v1.2)', () => {
  it('memetakan tipe poll, quiz, dan sticker', () => {
    expect(messageTypeLabel('poll')).toBe('Poll')
    expect(messageTypeLabel('quiz')).toBe('Kuis')
    expect(messageTypeLabel('sticker')).toBe('Stiker')
  })

  it('default ke Teks untuk tipe tidak dikenal atau kosong', () => {
    expect(messageTypeLabel(undefined)).toBe('Teks')
    expect(messageTypeLabel('text')).toBe('Teks')
  })
})

describe('formatPrice (v1.2)', () => {
  it('menampilkan XTR sebagai Stars', () => {
    expect(formatPrice(100, 'XTR')).toBe('100 ⭐ Stars')
    expect(formatPrice(50)).toBe('50 ⭐ Stars')
  })

  it('menampilkan mata uang lain apa adanya', () => {
    expect(formatPrice(15000, 'IDR')).toBe('15000 IDR')
  })
})

describe('conversationTitle / conversationInitials (v1.2)', () => {
  it('memakai title bila ada', () => {
    const c = { chat_id: 1, title: 'Grup Toko', username: 'toko', first_name: 'Toko' }
    expect(conversationTitle(c)).toBe('Grup Toko')
    expect(conversationInitials(c)).toBe('GT')
  })

  it('jatuh ke first_name + username bila tanpa title', () => {
    const c = { chat_id: 2, first_name: 'Budi', username: 'budi' }
    expect(conversationTitle(c)).toBe('Budi (@budi)')
    expect(conversationInitials(c)).toBe('BB')
  })

  it('jatuh ke Chat {id} bila tanpa nama apa pun', () => {
    expect(conversationTitle({ chat_id: 99 })).toBe('Chat 99')
  })
})
