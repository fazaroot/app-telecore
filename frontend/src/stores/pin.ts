// PIN app lock lokal (level aplikasi, BUKAN hardware-backed).
// Hanya hash SHA-256 + salt yang disimpan, tidak pernah PIN mentah.
// Penyimpanan: @capacitor/preferences di native, localStorage di web.
import { Preferences } from '@capacitor/preferences'
import { Capacitor } from '@capacitor/core'
import { create } from 'zustand'

const PIN_HASH_KEY = 'telecore_pin_hash'
const PIN_SALT_KEY = 'telecore_pin_salt'

function isNative(): boolean {
  try {
    return Capacitor.isNativePlatform()
  } catch {
    return false
  }
}

async function prefGet(key: string): Promise<string | null> {
  try {
    if (isNative()) {
      const { value } = await Preferences.get({ key })
      return value
    }
  } catch {
    // jatuh ke localStorage
  }
  return window.localStorage.getItem(key)
}

async function prefSet(key: string, value: string): Promise<void> {
  try {
    if (isNative()) {
      await Preferences.set({ key, value })
      return
    }
  } catch {
    // jatuh ke localStorage
  }
  window.localStorage.setItem(key, value)
}

async function prefRemove(key: string): Promise<void> {
  try {
    if (isNative()) await Preferences.remove({ key })
  } catch {
    // abaikan
  }
  window.localStorage.removeItem(key)
}

function bytesToHex(bytes: ArrayBuffer): string {
  return Array.from(new Uint8Array(bytes))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

async function sha256Hex(input: string): Promise<string> {
  const data = new TextEncoder().encode(input)
  const hash = await crypto.subtle.digest('SHA-256', data)
  return bytesToHex(hash)
}

function randomSalt(): string {
  const arr = new Uint8Array(16)
  crypto.getRandomValues(arr)
  return Array.from(arr)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

export async function isPinSet(): Promise<boolean> {
  const hash = await prefGet(PIN_HASH_KEY)
  return Boolean(hash)
}

export async function setPinLocal(pin: string): Promise<void> {
  const salt = randomSalt()
  const hash = await sha256Hex(`${salt}:${pin}`)
  await prefSet(PIN_SALT_KEY, salt)
  await prefSet(PIN_HASH_KEY, hash)
}

export async function verifyPinLocal(pin: string): Promise<boolean> {
  const [salt, hash] = await Promise.all([prefGet(PIN_SALT_KEY), prefGet(PIN_HASH_KEY)])
  if (!salt || !hash) return false
  const candidate = await sha256Hex(`${salt}:${pin}`)
  return candidate === hash
}

export async function clearPinLocal(): Promise<void> {
  await Promise.all([prefRemove(PIN_HASH_KEY), prefRemove(PIN_SALT_KEY)])
}

interface PinState {
  pinSet: boolean
  locked: boolean
  initialized: boolean
  lastActiveAt: number
  init: () => Promise<void>
  unlock: () => void
  lock: () => void
  refreshPinSet: () => Promise<void>
  touch: () => void
}

export const usePinStore = create<PinState>()((set, get) => ({
  pinSet: false,
  locked: false,
  initialized: false,
  lastActiveAt: Date.now(),
  init: async () => {
    const pinSet = await isPinSet()
    set({ pinSet, locked: pinSet, initialized: true, lastActiveAt: Date.now() })
  },
  unlock: () => set({ locked: false, lastActiveAt: Date.now() }),
  lock: () => {
    if (get().pinSet) set({ locked: true })
  },
  refreshPinSet: async () => {
    const pinSet = await isPinSet()
    set({ pinSet, locked: pinSet ? get().locked : false })
  },
  touch: () => set({ lastActiveAt: Date.now() }),
}))
