import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export const DEFAULT_BASE_URL = 'http://127.0.0.1:8080'

export interface SettingsState {
  baseUrl: string
  apiKey: string
  theme: 'light' | 'dark'
  language: 'id'
  autoLockMinutes: number
  setBaseUrl: (url: string) => void
  setApiKey: (key: string) => void
  setTheme: (theme: 'light' | 'dark') => void
  toggleTheme: () => void
  setAutoLockMinutes: (minutes: number) => void
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      baseUrl: DEFAULT_BASE_URL,
      apiKey: '',
      theme: 'light',
      language: 'id',
      autoLockMinutes: 5,
      setBaseUrl: (baseUrl) => set({ baseUrl }),
      setApiKey: (apiKey) => set({ apiKey }),
      setTheme: (theme) => set({ theme }),
      toggleTheme: () => set((s) => ({ theme: s.theme === 'light' ? 'dark' : 'light' })),
      setAutoLockMinutes: (autoLockMinutes) => set({ autoLockMinutes }),
    }),
    { name: 'telecore-settings' },
  ),
)

export interface UiState {
  drawerOpen: boolean
  setDrawerOpen: (open: boolean) => void
  toasts: { id: number; message: string; kind: 'success' | 'error' | 'info' }[]
  pushToast: (message: string, kind?: 'success' | 'error' | 'info') => void
  dismissToast: (id: number) => void
  backendReachable: boolean | null
  setBackendReachable: (ok: boolean | null) => void
}

let toastId = 0

export const useUiStore = create<UiState>()((set) => ({
  drawerOpen: false,
  setDrawerOpen: (drawerOpen) => set({ drawerOpen }),
  toasts: [],
  pushToast: (message, kind = 'info') => {
    const id = ++toastId
    set((s) => ({ toasts: [...s.toasts, { id, message, kind }] }))
    window.setTimeout(() => {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }))
    }, 4500)
  },
  dismissToast: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  backendReachable: null,
  setBackendReachable: (backendReachable) => set({ backendReachable }),
}))
