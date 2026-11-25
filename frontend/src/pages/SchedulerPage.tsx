import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CalendarClock, History, Pause, Play, Plus, Trash2 } from 'lucide-react'
import { fetchBots } from '../api/bots'
import {
  createTask,
  deleteTask,
  fetchSchedulerHistory,
  fetchTasks,
  pauseTask,
  resumeTask,
  updateTask,
} from '../api/automation'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { ConfirmDialog, Modal } from '../components/Modal'
import type { ScheduledTask } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

const TIMEZONES = [
  'Asia/Jakarta',
  'Asia/Makassar',
  'Asia/Jayapura',
  'Asia/Singapore',
  'Asia/Kuala_Lumpur',
  'UTC',
]

const DAYS = ['Minggu', 'Senin', 'Selasa', 'Rabu', 'Kamis', 'Jumat', 'Sabtu']

function taskDateKey(t: ScheduledTask): string {
  const raw = t.next_run_at ?? t.run_at ?? t.created_at
  if (!raw) return 'Tanpa tanggal'
  const d = new Date(raw)
  if (Number.isNaN(d.getTime())) return 'Tanpa tanggal'
  return d.toLocaleDateString('id-ID', { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' })
}

/** "HH:MM" -> [menit, jam] untuk cron "M H * * *". */
function timeToCronParts(time: string): [number, number] {
  const [h, m] = time.split(':').map((x) => Number(x))
  return [Number.isFinite(m) ? m : 0, Number.isFinite(h) ? h : 9]
}

/** ISO (UTC) -> nilai input datetime-local lokal "YYYY-MM-DDTHH:MM". */
function toLocalInput(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return ''
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** Cron "M H * * DOW" -> {time "HH:MM", dayOfWeek} bila sesuai pola. */
function cronToTimeDay(cron: string): { time: string; dayOfWeek: number } | null {
  const parts = cron.trim().split(/\s+/)
  if (parts.length < 5) return null
  const [mm, hh, , , dow] = parts
  const m = Number(mm)
  const h = Number(hh)
  if (!Number.isFinite(m) || !Number.isFinite(h)) return null
  const pad = (n: number) => String(n).padStart(2, '0')
  return { time: `${pad(h)}:${pad(m)}`, dayOfWeek: dow === '*' ? 1 : Number(dow) || 1 }
}

export function SchedulerPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<ScheduledTask | null>(null)
  const [deleting, setDeleting] = useState<ScheduledTask | null>(null)
  const [historyFor, setHistoryFor] = useState<ScheduledTask | null>(null)

  const [taskBotId, setTaskBotId] = useState('')
  const [taskName, setTaskName] = useState('')
  const [scheduleType, setScheduleType] = useState('one_time')
  const [runAt, setRunAt] = useState('')
  const [time, setTime] = useState('09:00')
  const [dayOfWeek, setDayOfWeek] = useState(1)
  const [intervalMinutes, setIntervalMinutes] = useState(60)
  const [timezone, setTimezone] = useState('Asia/Jakarta')
  const [chatId, setChatId] = useState('')
  const [message, setMessage] = useState('')

  const botsQuery = useQuery({ queryKey: ['bots'], queryFn: fetchBots })
  const tasksQuery = useQuery({ queryKey: ['scheduler-tasks'], queryFn: () => fetchTasks() })
  const historyQuery = useQuery({
    queryKey: ['scheduler-history', historyFor?.id],
    queryFn: () => fetchSchedulerHistory(historyFor!.id),
    enabled: historyFor !== null,
  })

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['scheduler-tasks'] })

  const saveMutation = useMutation({
    mutationFn: () => {
      // Petakan form ke field backend: task_type, message_content,
      // cron_expression ("M H * * *"), interval_seconds, next_run_at (RFC3339).
      const payload: Partial<ScheduledTask> = {
        bot_id: taskBotId || undefined,
        name: taskName.trim(),
        task_type: scheduleType === 'one_time' ? 'one-time' : scheduleType,
        timezone,
        chat_id: chatId.trim(),
        message_content: message.trim(),
      }
      if (scheduleType === 'one_time' && runAt) {
        const d = new Date(runAt)
        if (!Number.isNaN(d.getTime())) payload.next_run_at = d.toISOString()
      } else if (scheduleType === 'daily' || scheduleType === 'weekly') {
        const [mm, hh] = timeToCronParts(time)
        payload.cron_expression =
          scheduleType === 'weekly' ? `${mm} ${hh} * * ${dayOfWeek}` : `${mm} ${hh} * * *`
      } else if (scheduleType === 'interval') {
        payload.interval_seconds = intervalMinutes * 60
      }
      return editing ? updateTask(editing.id, payload) : createTask(payload)
    },
    onSuccess: () => {
      pushToast(editing ? 'Task diperbarui.' : 'Task dibuat.', 'success')
      setShowForm(false)
      setEditing(null)
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const pauseMutation = useMutation({
    mutationFn: (t: ScheduledTask) => (t.is_paused ? resumeTask(t.id) : pauseTask(t.id)),
    onSuccess: () => {
      pushToast('Status task diubah.', 'success')
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string | number) => deleteTask(id),
    onSuccess: () => {
      pushToast('Task dihapus.', 'success')
      setDeleting(null)
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const openForm = (t?: ScheduledTask) => {
    setEditing(t ?? null)
    setTaskBotId(t?.bot_id !== undefined ? String(t.bot_id) : '')
    setTaskName(t?.name ?? '')
    const backendType = t?.task_type ?? t?.schedule_type
    setScheduleType(backendType === 'one-time' ? 'one_time' : (backendType ?? 'one_time'))
    setRunAt(t?.next_run_at ? toLocalInput(t.next_run_at) : t?.run_at ? t.run_at.slice(0, 16) : '')
    const cron = t?.cron_expression ?? t?.cron
    const parsed = cron ? cronToTimeDay(cron) : null
    setTime(parsed?.time ?? t?.time ?? '09:00')
    setDayOfWeek(
      parsed?.dayOfWeek ?? (typeof t?.day_of_week === 'number' ? t.day_of_week : 1),
    )
    setIntervalMinutes(
      t?.interval_seconds !== undefined
        ? Math.max(1, Math.round(t.interval_seconds / 60))
        : (t?.interval_minutes ?? 60),
    )
    setTimezone(t?.timezone ?? 'Asia/Jakarta')
    setChatId(t?.chat_id !== undefined ? String(t.chat_id) : '')
    setMessage(t?.message_content ?? t?.message ?? t?.text ?? '')
    setShowForm(true)
  }

  const bots = botsQuery.data ?? []
  const tasks = tasksQuery.data ?? []
  const history = historyQuery.data ?? []

  const groupedByDate = useMemo(() => {
    const map = new Map<string, ScheduledTask[]>()
    for (const t of tasks) {
      const key = taskDateKey(t)
      map.set(key, [...(map.get(key) ?? []), t])
    }
    return [...map.entries()]
  }, [tasks])

  if (tasksQuery.isLoading) return <LoadingSpinner label="Memuat task terjadwal…" />
  if (tasksQuery.isError) {
    return (
      <div>
        <PageHeader title="Scheduler" subtitle="Pesan terjadwal untuk bot Anda" />
        <ErrorState message={(tasksQuery.error as Error).message} onRetry={() => void tasksQuery.refetch()} />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <PageHeader
        title="Scheduler"
        subtitle="Pesan terjadwal untuk bot Anda"
        action={
          <button
            type="button"
            onClick={() => openForm()}
            className="flex items-center gap-1 rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
          >
            <Plus className="h-4 w-4" /> Task
          </button>
        }
      />

      {tasks.length === 0 ? (
        <EmptyState
          icon={<CalendarClock className="h-10 w-10 text-primary" />}
          title="Belum ada task terjadwal"
          description="Buat task untuk mengirim pesan otomatis sekali, harian, mingguan, atau berkala."
          action={
            <button
              type="button"
              onClick={() => openForm()}
              className="mt-2 rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
            >
              Buat Task
            </button>
          }
        />
      ) : (
        <>
          <div className="space-y-3">
            {tasks.map((t) => (
              <Card key={String(t.id)}>
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="font-bold">{t.name ?? 'Task tanpa nama'}</p>
                    <p className="text-sm text-ink/60 dark:text-darktext/60">
                      {t.task_type ?? t.schedule_type ?? '—'}
                      {t.next_run_at ? ` · Berikutnya: ${new Date(t.next_run_at).toLocaleString('id-ID')}` : ''}
                      {t.timezone ? ` (${t.timezone})` : ''}
                    </p>
                    <p className="mt-1 truncate text-sm">{t.message_content ?? t.message ?? t.text ?? ''}</p>
                    <p className="text-xs text-ink/50 dark:text-darktext/50">Chat: {t.chat_id ?? '—'}</p>
                  </div>
                  <Badge tone={t.is_paused ? 'warning' : 'success'}>{t.is_paused ? 'Jeda' : 'Aktif'}</Badge>
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => pauseMutation.mutate(t)}
                    disabled={pauseMutation.isPending}
                    className="flex items-center gap-1 rounded-full bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary"
                  >
                    {t.is_paused ? <Play className="h-3.5 w-3.5" /> : <Pause className="h-3.5 w-3.5" />}
                    {t.is_paused ? 'Lanjutkan' : 'Jeda'}
                  </button>
                  <button
                    type="button"
                    onClick={() => openForm(t)}
                    className="rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    onClick={() => setHistoryFor(t)}
                    className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
                  >
                    <History className="h-3.5 w-3.5" /> Riwayat
                  </button>
                  <button
                    type="button"
                    onClick={() => setDeleting(t)}
                    className="flex items-center gap-1 rounded-full bg-danger/10 px-3 py-1.5 text-xs font-semibold text-danger"
                  >
                    <Trash2 className="h-3.5 w-3.5" /> Hapus
                  </button>
                </div>
              </Card>
            ))}
          </div>

          <Card>
            <h2 className="mb-2 font-bold">Task per Tanggal</h2>
            <p className="mb-3 text-xs text-ink/50 dark:text-darktext/50">
              Daftar task dikelompokkan per tanggal eksekusi berikutnya.
            </p>
            <div className="space-y-3">
              {groupedByDate.map(([date, list]) => (
                <div key={date}>
                  <p className="text-sm font-semibold text-primary">{date}</p>
                  <ul className="mt-1 space-y-1">
                    {list.map((t) => (
                      <li key={String(t.id)} className="text-sm text-ink/80 dark:text-darktext/80">
                        • {t.name ?? 'Task'} — {t.message_content ?? t.message ?? t.text ?? ''} {t.is_paused ? '(dijeda)' : ''}
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
            </div>
          </Card>
        </>
      )}

      <Modal open={showForm} onClose={() => setShowForm(false)} title={editing ? 'Edit Task' : 'Task Baru'}>
        <div className="space-y-3">
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Bot</span>
            <select value={taskBotId} onChange={(e) => setTaskBotId(e.target.value)} className={inputCls}>
              <option value="">Pilih bot…</option>
              {bots.map((b) => (
                <option key={String(b.id)} value={String(b.id)}>
                  {b.name ?? b.username ?? `Bot ${b.id}`}
                </option>
              ))}
            </select>
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Nama task</span>
            <input value={taskName} onChange={(e) => setTaskName(e.target.value)} placeholder="mis. Pengingat pagi" className={inputCls} />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Jenis jadwal</span>
            <select value={scheduleType} onChange={(e) => setScheduleType(e.target.value)} className={inputCls}>
              <option value="one_time">Sekali (one-time)</option>
              <option value="daily">Harian</option>
              <option value="weekly">Mingguan</option>
              <option value="interval">Interval berkala</option>
            </select>
          </label>
          {scheduleType === 'one_time' && (
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Tanggal & waktu</span>
              <input type="datetime-local" value={runAt} onChange={(e) => setRunAt(e.target.value)} className={inputCls} />
            </label>
          )}
          {(scheduleType === 'daily' || scheduleType === 'weekly') && (
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Waktu</span>
              <input type="time" value={time} onChange={(e) => setTime(e.target.value)} className={inputCls} />
            </label>
          )}
          {scheduleType === 'weekly' && (
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Hari</span>
              <select value={dayOfWeek} onChange={(e) => setDayOfWeek(Number(e.target.value))} className={inputCls}>
                {DAYS.map((d, i) => (
                  <option key={i} value={i}>{d}</option>
                ))}
              </select>
            </label>
          )}
          {scheduleType === 'interval' && (
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Interval (menit)</span>
              <input
                type="number"
                min={1}
                value={intervalMinutes}
                onChange={(e) => setIntervalMinutes(Math.max(1, Number(e.target.value) || 60))}
                className={inputCls}
              />
            </label>
          )}
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Zona waktu</span>
            <select value={timezone} onChange={(e) => setTimezone(e.target.value)} className={inputCls}>
              {TIMEZONES.map((tz) => (
                <option key={tz} value={tz}>{tz}</option>
              ))}
            </select>
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Chat ID tujuan</span>
            <input value={chatId} onChange={(e) => setChatId(e.target.value)} placeholder="mis. 123456789" className={inputCls} />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Pesan</span>
            <textarea value={message} onChange={(e) => setMessage(e.target.value)} rows={3} placeholder="Isi pesan yang dikirim terjadwal" className={inputCls} />
          </label>
          <button
            type="button"
            disabled={!taskBotId || !taskName.trim() || !chatId.trim() || !message.trim() || saveMutation.isPending}
            onClick={() => saveMutation.mutate()}
            className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
          >
            {saveMutation.isPending ? 'Menyimpan…' : 'Simpan Task'}
          </button>
        </div>
      </Modal>

      <Modal open={historyFor !== null} onClose={() => setHistoryFor(null)} title={`Riwayat — ${historyFor?.name ?? ''}`}>
        {historyQuery.isLoading ? (
          <LoadingSpinner label="Memuat riwayat task…" />
        ) : historyQuery.isError ? (
          <ErrorState message={(historyQuery.error as Error).message} onRetry={() => void historyQuery.refetch()} />
        ) : history.length === 0 ? (
          <EmptyState title="Belum ada riwayat" description="Task ini belum pernah dieksekusi." />
        ) : (
          <ul className="space-y-2">
            {history.map((h) => (
              <li key={String(h.id)} className="rounded-xl border border-ink/10 p-3 text-sm dark:border-darktext/10">
                <div className="flex items-center justify-between gap-2">
                  <Badge tone={h.status === 'failed' || h.status === 'error' ? 'danger' : 'success'}>
                    {h.status ?? '—'}
                  </Badge>
                  <span className="text-xs text-ink/50 dark:text-darktext/50">
                    {h.run_at ? new Date(h.run_at).toLocaleString('id-ID') : ''}
                  </span>
                </div>
                {h.error && <p className="mt-1 text-xs text-danger">{h.error}</p>}
              </li>
            ))}
          </ul>
        )}
      </Modal>

      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && deleteMutation.mutate(deleting.id)}
        title="Hapus Task"
        message={`Hapus task "${deleting?.name ?? ''}"?`}
        busy={deleteMutation.isPending}
      />
    </div>
  )
}
