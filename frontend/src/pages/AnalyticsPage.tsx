import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Download, Printer } from 'lucide-react'
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import { exportAnalytics, fetchAnalyticsSummary } from '../api/misc'
import { useUiStore } from '../stores/settings'
import { Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

function defaultRange(): { from: string; to: string } {
  const to = new Date()
  const from = new Date()
  from.setDate(from.getDate() - 30)
  const fmt = (d: Date) => d.toISOString().slice(0, 10)
  return { from: fmt(from), to: fmt(to) }
}

export function AnalyticsPage() {
  const pushToast = useUiStore((s) => s.pushToast)
  const [{ from, to }, setRange] = useState(defaultRange)
  const [exporting, setExporting] = useState(false)

  const query = useQuery({
    queryKey: ['analytics', 'summary', from, to],
    queryFn: () => fetchAnalyticsSummary(from, to),
  })

  const data = query.data
  const daily = useMemo(
    () =>
      (data?.daily ?? []).map((d) => ({
        date: (d.date ?? '').slice(5),
        masuk: d.messages_in ?? 0,
        keluar: d.messages_out ?? 0,
        pengguna: d.users ?? 0,
      })),
    [data],
  )
  const commandUsage = useMemo(
    () =>
      (data?.command_usage ?? []).map((c) => ({
        command: c.command ?? '—',
        jumlah: c.count ?? 0,
      })),
    [data],
  )

  const handleExport = async (format: 'csv' | 'json') => {
    setExporting(true)
    try {
      await exportAnalytics(format, from, to)
      pushToast(`Analytics diekspor sebagai ${format.toUpperCase()}.`, 'success')
    } catch (e) {
      pushToast((e as Error).message, 'error')
    } finally {
      setExporting(false)
    }
  }

  if (query.isLoading) return <LoadingSpinner label="Memuat analytics…" />
  if (query.isError) {
    return (
      <div>
        <PageHeader title="Analytics" subtitle="Statistik bot dan pengguna" />
        <ErrorState message={(query.error as Error).message} onRetry={() => void query.refetch()} />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <PageHeader
        title="Analytics"
        subtitle="Statistik bot dan pengguna"
        action={
          <button
            type="button"
            onClick={() => window.print()}
            className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-2 text-sm font-semibold dark:bg-darktext/10"
          >
            <Printer className="h-4 w-4" /> Cetak / Simpan PDF
          </button>
        }
      />
      <p className="-mt-2 text-xs text-ink/50 dark:text-darktext/50">
        Tombol cetak memakai print browser (via print browser) — pilih "Save as PDF" di dialog cetak.
      </p>

      <div className="print-hidden flex flex-wrap items-end gap-2">
        <label className="text-sm">
          <span className="mb-1 block font-medium text-ink/70 dark:text-darktext/70">Dari</span>
          <input
            type="date"
            value={from}
            onChange={(e) => setRange((r) => ({ ...r, from: e.target.value }))}
            className={inputCls}
          />
        </label>
        <label className="text-sm">
          <span className="mb-1 block font-medium text-ink/70 dark:text-darktext/70">Sampai</span>
          <input
            type="date"
            value={to}
            onChange={(e) => setRange((r) => ({ ...r, to: e.target.value }))}
            className={inputCls}
          />
        </label>
        <button
          type="button"
          onClick={() => handleExport('csv')}
          disabled={exporting}
          className="flex items-center gap-1 rounded-full bg-primary/10 px-3 py-2 text-sm font-semibold text-primary disabled:opacity-50"
        >
          <Download className="h-4 w-4" /> Export CSV
        </button>
        <button
          type="button"
          onClick={() => handleExport('json')}
          disabled={exporting}
          className="flex items-center gap-1 rounded-full bg-primary/10 px-3 py-2 text-sm font-semibold text-primary disabled:opacity-50"
        >
          <Download className="h-4 w-4" /> Export JSON
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <Card>
          <p className="text-sm text-ink/60 dark:text-darktext/60">Pengguna aktif</p>
          <p className="mt-1 text-2xl font-bold">{data?.active_users ?? 0}</p>
        </Card>
        <Card>
          <p className="text-sm text-ink/60 dark:text-darktext/60">Pengguna baru</p>
          <p className="mt-1 text-2xl font-bold">{data?.new_users ?? 0}</p>
        </Card>
        <Card>
          <p className="text-sm text-ink/60 dark:text-darktext/60">Engagement</p>
          <p className="mt-1 text-2xl font-bold">
            {data?.engagement_rate !== undefined ? `${data.engagement_rate}%` : '—'}
          </p>
        </Card>
        <Card>
          <p className="text-sm text-ink/60 dark:text-darktext/60">Automation berhasil</p>
          <p className="mt-1 text-2xl font-bold">
            {data?.automation_success_rate !== undefined ? `${data.automation_success_rate}%` : '—'}
          </p>
          <p className="text-xs text-ink/50 dark:text-darktext/50">{data?.automation_runs ?? 0} eksekusi</p>
        </Card>
        <Card>
          <p className="text-sm text-ink/60 dark:text-darktext/60">Request gagal</p>
          <p className="mt-1 text-2xl font-bold text-danger">{data?.failed_requests ?? 0}</p>
        </Card>
        <Card>
          <p className="text-sm text-ink/60 dark:text-darktext/60">Waktu respons rata-rata</p>
          <p className="mt-1 text-2xl font-bold">
            {data?.avg_response_ms !== undefined ? `${data.avg_response_ms} ms` : '—'}
          </p>
        </Card>
      </div>

      <Card>
        <h2 className="mb-2 font-bold">Pesan per Hari</h2>
        {daily.length === 0 ? (
          <EmptyState title="Belum ada data harian" description="Data muncul setelah ada pesan tercatat dalam rentang tanggal ini." />
        ) : (
          <div className="h-56 w-full">
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={daily}>
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
        <h2 className="mb-2 font-bold">Penggunaan Command</h2>
        {commandUsage.length === 0 ? (
          <EmptyState title="Belum ada data command" description="Command yang dipakai pengguna akan muncul di sini." />
        ) : (
          <div className="h-56 w-full">
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={commandUsage}>
                <CartesianGrid strokeDasharray="3 3" opacity={0.3} />
                <XAxis dataKey="command" fontSize={11} />
                <YAxis fontSize={11} />
                <Tooltip />
                <Bar dataKey="jumlah" fill="#7187ED" radius={[4, 4, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          </div>
        )}
      </Card>
    </div>
  )
}
