import { useEffect } from 'react'
import { HashRouter, Route, Routes } from 'react-router-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Layout } from './components/Layout'
import { ErrorBoundary } from './components/ErrorBoundary'
import { PinLockScreen } from './components/PinLock'
import { usePinStore } from './stores/pin'
import { useSettingsStore } from './stores/settings'
import { syncStatusBar } from './utils/statusBar'
import { DashboardPage } from './pages/DashboardPage'
import { BotsPage } from './pages/BotsPage'
import { InboxPage } from './pages/InboxPage'
import { PaymentsPage } from './pages/PaymentsPage'
import { ChannelsPage } from './pages/ChannelsPage'
import { MessagesPage } from './pages/MessagesPage'
import { AutomationPage } from './pages/AutomationPage'
import { UsersPage } from './pages/UsersPage'
import { GroupsPage } from './pages/GroupsPage'
import { SchedulerPage } from './pages/SchedulerPage'
import { AnalyticsPage } from './pages/AnalyticsPage'
import { NotificationsPage } from './pages/NotificationsPage'
import { SecurityPage } from './pages/SecurityPage'
import { SettingsPage } from './pages/SettingsPage'
import { IntegrationsPage } from './pages/IntegrationsPage'
import { LogsPage } from './pages/LogsPage'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 10_000,
      refetchOnWindowFocus: false,
    },
  },
})

export default function App() {
  const initialized = usePinStore((s) => s.initialized)
  const locked = usePinStore((s) => s.locked)
  const init = usePinStore((s) => s.init)
  const theme = useSettingsStore((s) => s.theme)

  useEffect(() => {
    void init()
  }, [init])

  // StatusBar native: matikan overlay + sesuaikan style/background dengan tema.
  // Dijalankan sekali saat start dan setiap kali tema berubah; aman di web.
  useEffect(() => {
    void syncStatusBar(theme)
  }, [theme])

  if (!initialized) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-bg dark:bg-darkbg">
        <p className="text-lg font-bold text-ink dark:text-darktext">
          Tele<span className="text-primary">Core</span>
        </p>
      </div>
    )
  }

  if (locked) return <PinLockScreen />

  return (
    <QueryClientProvider client={queryClient}>
      <HashRouter>
        <ErrorBoundary>
          <Routes>
          <Route element={<Layout />}>
            <Route index element={<DashboardPage />} />
            <Route path="bots" element={<BotsPage />} />
            <Route path="inbox" element={<InboxPage />} />
            <Route path="messages" element={<MessagesPage />} />
            <Route path="payments" element={<PaymentsPage />} />
            <Route path="channels" element={<ChannelsPage />} />
            <Route path="automation" element={<AutomationPage />} />
            <Route path="users" element={<UsersPage />} />
            <Route path="groups" element={<GroupsPage />} />
            <Route path="scheduler" element={<SchedulerPage />} />
            <Route path="analytics" element={<AnalyticsPage />} />
            <Route path="integrations" element={<IntegrationsPage />} />
            <Route path="logs" element={<LogsPage />} />
            <Route path="notifications" element={<NotificationsPage />} />
            <Route path="security" element={<SecurityPage />} />
            <Route path="settings" element={<SettingsPage />} />
            <Route path="*" element={<DashboardPage />} />
          </Route>
        </Routes>
        </ErrorBoundary>
      </HashRouter>
    </QueryClientProvider>
  )
}
