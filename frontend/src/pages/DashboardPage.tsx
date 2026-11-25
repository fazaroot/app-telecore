import { useEffect, useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Activity, ArrowDownLeft, ArrowUpRight, Bot, CheckCircle2, Server, Users, Workflow, XCircle } from 'lucide-react'
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { fetchHealth, fetchBots } from '../api/bots'
import { fetchAnalyticsSummary } from '../api/misc'
import { fetchWorkflows } from '../api/automation'
import { fetchMessages } from '../api/messages'
import { useWebSocket } from '../hooks/useWebSocket'
import { isImportantEvent, scheduleLocalNotification } from '../hooks/useLocalNotifications'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import type { ActivityItem, WsEvent } from '../types'

function eventToActivity(ev: WsEvent): ActivityItem | null {
  const p = (ev.payload ?? {}) as Record<string, unknown>
  const text =
    (typeof p.text === 'string' && p.text) ||
    (typeof p.message === 'string' && p.message) ||
    (typeof p.title === 'string' && p.title) ||
    null
  if (!text) return null
  return { id: `${ev.ts ?? Date.now()}-${Math.random()}`, text, ts: ev.ts ?? new Date().toISOString(), kind: ev.type }
}

export function DashboardPage() {
  const [activities, setActivities] = useState<ActivityItem[]>([])
  const { status: wsStatus } = useWebSocket((ev) => {
    const item = eventToActivity(ev)
    if (item) setActivities((prev) => [item, ...prev].slice(0, 30))
    const important = isImportantEvent(ev.type, ev.payload)
    if (important) void scheduleLocalNotification(important.title, important.body)
  })

  const healthQuery = useQuery({ queryKey: ['health'], queryFn: fetchHealth, refetchInterval: 30_000 })
  const botsQuery = useQuery({ queryKey: ['bots'], queryFn: fetchBots, refetchInterval: wsStatus === 'open' ? false : 10_000 })
  const analyticsQuery = useQuery({ queryKey: ['analytics', 'summary'], queryFn: () => fetchAnalyticsSummary(), refetchInterval: wsStatus === 'open' ? false : 10_000 })
  const workflowsQuery = useQuery({ queryKey: ['workflows'], queryFn: () => fetchWorkflows(), refetchInterval: wsStatus === 'open' ? false : 10_000 })
  const messagesQuery = useQuery({ queryKey: ['messages', 'recent'], queryFn: () => fetchMessages({}), refetchInterval: wsStatus === 'open' ? false : 10_000 })

  const bots = botsQuery.data ?? []
  const analytics = analyticsQuery.data
  const workflows = workflowsQuery.data ?? []
  const messages = messagesQuery.data ?? []

  const activeBots = bots.filter((b) => b.is_active).length
  const connectedBots = bots.filter((b) => b.connection_status === 'connected').length
  const messagesIn =
    analytics?.messages_in ?? messages.filter((m) => m.direction === 'in' || m.direction === 'inbound').length
  const messagesOut =
    analytics?.messages_out ??
    messages.filter((m) => m.direction === 'out' || m.direction === 'outbound').length
  const totalUsers = analytics?.total_users ?? 0
  const enabledWorkflows = workflows.filter((w) => w.is_enabled).length

  const chartData = useMemo(() => {
    const daily = analytics?.daily ?? []
    return daily.map((d) => ({
      date: (d.date ?? '').slice(5),
      masuk: d.messages_in ?? 0,
      keluar: d.messages_out ?? 0,
    }))
  }, [analytics])

  // Seed activity feed dari pesan terbaru saat WS belum mengirim apa-apa.
  useEffect(() => {
    if (activities.length === 0 && messages.length > 0) {
      setActivities(
        messages.slice(0, 10).map((m) => ({
          id: String(m.id),
          text: `Pesan ${m.direction === 'in' || m.direction === 'inbound' ? 'masuk' : 'keluar'} — ${(m.text ?? m.content) ? (m.text ?? m.content)?.slice(0, 60) : (m.message_type ?? 'media')} (status: ${m.status ?? '—'})`,
          ts: m.created_at ?? new Date().toISOString(),
          kind: 'message',
        })),
      )
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [messages.length])

  if (healthQuery.isLoading && botsQuery.isLoading) return <LoadingSpinner label="Memuat dashboard…" />
  if (healthQuery.isError && botsQuery.isError) {
    return (
      <div>
        <PageHeader title="Dashboard" subtitle="Ringkasan bot dan aktivitas Telegram Anda" />
        <ErrorState
          message="Backend tidak dapat dijangkau. Periksa alamat server dan API key di Settings."
          onRetry={() => {
            void healthQuery.refetch()
            void botsQuery.refetch()
          }}
        />
      </div>
    )
  }

  const health = healthQuery.data
  const dbField = health?.db ?? health?.database
  const dbOk = (dbField ?? '').toString().toLowerCase().includes('ok') || dbField === 'connected'

  return (
    <div className="space-y-4">
      <PageHeader
        title="Dashboard"
        subtitle="Ringkasan bot dan aktivitas Telegram Anda"
        action={
          <Badge tone={wsStatus === 'open' ? 'success' : 'warning'}>
            {wsStatus === 'open' ? 'Live (WebSocket)' : 'Polling 10 dtk'}
          </Badge>
        }
      />

      <div className="grid grid-cols-2 gap-3">
        <Card>
          <div className="flex items-center gap-2 text-sm text-ink/60 dark:text-darktext/60">
            <Bot className="h-4 w-4" /> Total Bot
          </div>
          <p className="mt-1 text-2xl font-bold">{bots.length}</p>
          <p className="text-xs text-ink/50 dark:text-darktext/50">
            {activeBots} aktif · {connectedBots} terhubung
          </p>
        </Card>
        <Card>
          <div className="flex items-center gap-2 text-sm text-ink/60 dark:text-darktext/60">
            <Users className="h-4 w-4" /> Total Pengguna
          </div>
          <p className="mt-1 text-2xl font-bold">{totalUsers}</p>
          <p className="text-xs text-ink/50 dark:text-darktext/50">dari analytics server</p>
        </Card>
        <Card>
          <div className="flex items-center gap-2 text-sm text-ink/60 dark:text-darktext/60">
            <ArrowDownLeft className="h-4 w-4" /> Pesan Masuk
          </div>
          <p className="mt-1 text-2xl font-bold">{messagesIn}</p>
        </Card>
        <Card>
          <div className="flex items-center gap-2 text-sm text-ink/60 dark:text-darktext/60">
            <ArrowUpRight className="h-4 w-4" /> Pesan Keluar
          </div>
          <p className="mt-1 text-2xl font-bold">{messagesOut}</p>
        </Card>
      </div>

      <Card>
        <div className="mb-2 flex items-center justify-between">
          <h2 className="font-bold">Status Sistem</h2>
          <Badge tone={healthQuery.isSuccess ? 'success' : 'danger'}>
            {healthQuery.isSuccess ? 'API terhubung' : 'API bermasalah'}
          </Badge>
        </div>
        <dl className="space-y-2 text-sm">
          <div className="flex justify-between">
            <dt className="flex items-center gap-2 text-ink/60 dark:text-darktext/60">
              <Server className="h-4 w-4" /> Server health
            </dt>
            <dd className="font-medium">{health?.status ?? '—'}</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-ink/60 dark:text-darktext/60">Database</dt>
            <dd className="flex items-center gap-1 font-medium">
              {dbOk ? <CheckCircle2 className="h-4 w-4 text-success" /> : <XCircle className="h-4 w-4 text-danger" />}
              {health?.db ?? '—'}
            </dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-ink/60 dark:text-darktext/60">Versi backend</dt>
            <dd className="font-medium">{health?.version ?? '—'}</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-ink/60 dark:text-darktext/60">Uptime</dt>
            <dd className="font-medium">{health?.uptime !== undefined ? String(health.uptime) : '—'}</dd>
          </div>
          <div className="flex justify-between">
            <dt className="flex items-center gap-2 text-ink/60 dark:text-darktext/60">
              <Workflow className="h-4 w-4" /> Automation aktif
            </dt>
            <dd className="font-medium">
              {enabledWorkflows} dari {workflows.length} workflow
            </dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-ink/60 dark:text-darktext/60">Pesan gagal</dt>
            <dd className="font-medium text-danger">
              {messages.filter((m) => m.status === 'failed').length} dari {messages.length} pesan terakhir
            </dd>
          </div>
        </dl>
      </Card>

      <Card>
        <h2 className="mb-2 font-bold">Traffic Pesan</h2>
        {chartData.length === 0 ? (
          <EmptyState title="Belum ada data traffic" description="Grafik muncul setelah ada pesan yang tercatat di analytics." />
        ) : (
          <div className="h-56 w-full">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={chartData}>
                <CartesianGrid strokeDasharray="3 3" opacity={0.3} />
                <XAxis dataKey="date" fontSize={11} />
                <YAxis fontSize={11} />
                <Tooltip />
                <Area type="monotone" dataKey="masuk" stroke="#526FE5" fill="#526FE5" fillOpacity={0.2} />
                <Area type="monotone" dataKey="keluar" stroke="#25A56A" fill="#25A56A" fillOpacity={0.2} />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        )}
      </Card>

      <Card>
        <h2 className="mb-2 flex items-center gap-2 font-bold">
          <Activity className="h-4 w-4" /> Aktivitas Live
        </h2>
        {activities.length === 0 ? (
          <EmptyState title="Belum ada aktivitas" description="Aktivitas terbaru dari WebSocket akan muncul di sini." />
        ) : (
          <ul className="space-y-2">
            {activities.map((a) => (
              <li key={a.id} className="flex items-start justify-between gap-3 border-b border-ink/5 pb-2 text-sm last:border-0 dark:border-darktext/5">
                <span>{a.text}</span>
                <span className="shrink-0 text-xs text-ink/50 dark:text-darktext/50">
                  {new Date(a.ts).toLocaleTimeString('id-ID', { hour: '2-digit', minute: '2-digit' })}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Card>
    </div>
  )
}
