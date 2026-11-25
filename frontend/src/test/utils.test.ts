import { describe, expect, it } from 'vitest'
import { buildKeyboardPayload } from '../components/KeyboardBuilder'
import { asArray } from '../api/client'

describe('buildKeyboardPayload', () => {
  it('mengembalikan undefined tanpa keyboard', () => {
    expect(buildKeyboardPayload('none', [])).toBeUndefined()
    expect(buildKeyboardPayload('inline', [])).toBeUndefined()
  })

  it('membuat payload inline keyboard dengan callback_data default dari teks tombol', () => {
    const payload = buildKeyboardPayload('inline', [[{ text: 'Ya' }, { text: 'Tidak', url: 'https://example.com' }]])
    expect(payload).toEqual({
      inline_keyboard: [
        [
          { text: 'Ya', callback_data: 'Ya' },
          // Telegram menolak tombol yang punya callback_data DAN url sekaligus.
          { text: 'Tidak', url: 'https://example.com' },
        ],
      ],
    })
  })

  it('membuat payload reply keyboard', () => {
    const payload = buildKeyboardPayload('reply', [[{ text: 'Halo' }]])
    expect(payload).toEqual({
      keyboard: [[{ text: 'Halo' }]],
      resize_keyboard: true,
      one_time_keyboard: false,
    })
  })
})

describe('asArray', () => {
  it('menerima array langsung', () => {
    expect(asArray([1, 2])).toEqual([1, 2])
  })

  it('mengambil array dari kunci pembungkus', () => {
    expect(asArray({ bots: [{ id: 1 }] }, ['bots', 'items'])).toEqual([{ id: 1 }])
  })

  it('mengembalikan array kosong untuk data tak dikenal', () => {
    expect(asArray(null)).toEqual([])
    expect(asArray({})).toEqual([])
  })
})
