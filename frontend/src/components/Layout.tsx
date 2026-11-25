import { useEffect } from 'react'
import { NavLink, Outlet, useLocation } from 'react-router-dom'
import { AnimatePresence, motion } from 'framer-motion'
import {
  BarChart3,
  Bell,
  Bot,
  CalendarClock,
  Home,
  Menu,
  MessageSquare,
  Moon,
  Settings,
  Shield,
  Sun,
  Users,
  UsersRound,
  WifiOff,
  Workflow,
  X,
} from 'lucide-react'
import { useSettingsStore, useUiStore } from '../stores/settings'
import { usePinStore } from '../stores/pin'
import { Toasts } from './Toast'

const NAV_ITEMS = [
  { to: '/', label: 'Dashboard', icon: Home },
  { to: '/bots', label: 'My Bots', icon: Bot },
  { to: '/messages', label: 'Messages', icon: MessageSquare },
  { to: '/automation', label: 'Automation', icon: Workflow },
  { to: '/users', label: 'Users', icon: Users },
  { to: '/groups', label: 'Groups', icon: UsersRound },
  { to: '/scheduler', label: 'Scheduler', icon: CalendarClock },
  { to: '/analytics', label: 'Analytics', icon: BarChart3 },
  { to: '/notifications', label: 'Notifications', icon: Bell },
  { to: '/security', label: 'Security', icon: Shield },
  { to: '/settings', label: 'Settings', icon: Settings },
]

const BOTTOM_NAV = [
  { to: '/', label: 'Dashboard', icon: Home },
  { to: '/bots', label: 'Bots', icon: Bot },
  { to: '/messages', label: 'Messages', icon: MessageSquare },
  { to: '/automation', label: 'Automation', icon: Workflow },
]

export function Layout() {
  const drawerOpen = useUiStore((s) => s.drawerOpen)
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen)
  const backendReachable = useUiStore((s) => s.backendReachable)
  const theme = useSettingsStore((s) => s.theme)
  const toggleTheme = useSettingsStore((s) => s.toggleTheme)
  const autoLockMinutes = useSettingsStore((s) => s.autoLockMinutes)
  const location = useLocation()

  // Terapkan class dark pada <html>
  useEffect(() => {
    document.documentElement.classList.toggle('dark', theme === 'dark')
  }, [theme])

  // Auto-lock: kunci aplikasi setelah idle sesuai pengaturan.
  useEffect(() => {
    const { lastActiveAt, lock, pinSet } = usePinStore.getState()
    if (!pinSet || autoLockMinutes <= 0) return
    const id = window.setInterval(() => {
      const idleMs = Date.now() - usePinStore.getState().lastActiveAt
      if (idleMs > autoLockMinutes * 60_000) lock()
    }, 30_000)
    void lastActiveAt
    return () => window.clearInterval(id)
  }, [autoLockMinutes])

  // Catat aktivitas pengguna untuk auto-lock.
  useEffect(() => {
    const touch = () => usePinStore.getState().touch()
    window.addEventListener('pointerdown', touch)
    window.addEventListener('keydown', touch)
    return () => {
      window.removeEventListener('pointerdown', touch)
      window.removeEventListener('keydown', touch)
    }
  }, [])

  // Tutup drawer setiap pindah halaman.
  useEffect(() => {
    setDrawerOpen(false)
  }, [location.pathname, setDrawerOpen])

  return (
    <div className="min-h-screen bg-bg text-ink dark:bg-darkbg dark:text-darktext">
      <header className="sticky top-0 z-40 flex items-center justify-between border-b border-ink/10 bg-surface/90 px-4 py-3 backdrop-blur dark:border-darktext/10 dark:bg-darksurface/90">
        <div className="flex items-center gap-2">
          <button
            type="button"
            aria-label="Buka menu"
            onClick={() => setDrawerOpen(true)}
            className="rounded-full p-2 hover:bg-ink/10 dark:hover:bg-darktext/10"
          >
            <Menu className="h-5 w-5" />
          </button>
          <span className="text-lg font-bold">
            Tele<span className="text-primary">Core</span>
          </span>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            aria-label={theme === 'dark' ? 'Mode terang' : 'Mode gelap'}
            onClick={toggleTheme}
            className="rounded-full p-2 hover:bg-ink/10 dark:hover:bg-darktext/10"
          >
            {theme === 'dark' ? <Sun className="h-5 w-5" /> : <Moon className="h-5 w-5" />}
          </button>
          <NavLink
            to="/notifications"
            aria-label="Notifikasi"
            className="rounded-full p-2 hover:bg-ink/10 dark:hover:bg-darktext/10"
          >
            <Bell className="h-5 w-5" />
          </NavLink>
        </div>
      </header>

      {backendReachable === false && (
        <div className="flex items-center gap-2 bg-danger/10 px-4 py-2 text-sm font-medium text-danger">
          <WifiOff className="h-4 w-4 shrink-0" />
          <span>Backend tidak terjangkau — menampilkan data terakhir yang tersimpan.</span>
        </div>
      )}

      <AnimatePresence>
        {drawerOpen && (
          <>
            <motion.div
              className="fixed inset-0 z-40 bg-black/50"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              onClick={() => setDrawerOpen(false)}
            />
            <motion.nav
              className="fixed inset-y-0 left-0 z-50 w-72 overflow-y-auto bg-surface p-4 dark:bg-darksurface"
              initial={{ x: -300 }}
              animate={{ x: 0 }}
              exit={{ x: -300 }}
              transition={{ type: 'tween', duration: 0.2 }}
            >
              <div className="mb-4 flex items-center justify-between">
                <span className="text-lg font-bold">
                  Tele<span className="text-primary">Core</span>
                </span>
                <button
                  type="button"
                  aria-label="Tutup menu"
                  onClick={() => setDrawerOpen(false)}
                  className="rounded-full p-1.5 hover:bg-ink/10 dark:hover:bg-darktext/10"
                >
                  <X className="h-5 w-5" />
                </button>
              </div>
              <div className="flex flex-col gap-1">
                {NAV_ITEMS.map((item) => (
                  <NavLink
                    key={item.to}
                    to={item.to}
                    end={item.to === '/'}
                    className={({ isActive }) =>
                      `flex items-center gap-3 rounded-xl px-3 py-2.5 text-sm font-medium ${
                        isActive
                          ? 'bg-primary/15 text-primary'
                          : 'text-ink/80 hover:bg-ink/5 dark:text-darktext/80'
                      }`
                    }
                  >
                    <item.icon className="h-5 w-5" />
                    {item.label}
                  </NavLink>
                ))}
              </div>
              <p className="mt-6 px-3 text-xs text-ink/50 dark:text-darktext/50">
                TeleCore v1.0.0 — Telegram Bot Control Center
              </p>
            </motion.nav>
          </>
        )}
      </AnimatePresence>

      <main className="mx-auto w-full max-w-3xl px-4 pb-28 pt-4">
        <AnimatePresence mode="wait">
          <motion.div
            key={location.pathname}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            transition={{ duration: 0.18 }}
          >
            <Outlet />
          </motion.div>
        </AnimatePresence>
      </main>

      <nav className="fixed inset-x-0 bottom-0 z-40 border-t border-ink/10 bg-surface/95 pb-[env(safe-area-inset-bottom)] backdrop-blur dark:border-darktext/10 dark:bg-darksurface/95">
        <div className="mx-auto grid max-w-3xl grid-cols-5">
          {BOTTOM_NAV.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === '/'}
              className={({ isActive }) =>
                `flex flex-col items-center gap-0.5 py-2.5 text-[11px] font-medium ${
                  isActive ? 'text-primary' : 'text-ink/60 dark:text-darktext/60'
                }`
              }
            >
              <item.icon className="h-5 w-5" />
              {item.label}
            </NavLink>
          ))}
          <button
            type="button"
            onClick={() => setDrawerOpen(true)}
            className="flex flex-col items-center gap-0.5 py-2.5 text-[11px] font-medium text-ink/60 dark:text-darktext/60"
          >
            <Menu className="h-5 w-5" />
            Lainnya
          </button>
        </div>
      </nav>

      <Toasts />
    </div>
  )
}
