import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Ban, Download, Search, User } from 'lucide-react'
import { fetchBots } from '../api/bots'
import { blockUser, exportUsersCsv, fetchUser, fetchUsers, unblockUser, updateUser } from '../api/users'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { Modal } from '../components/Modal'
import type { TelegramUser } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

export function UsersPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [filterBot, setFilterBot] = useState('')
  const [search, setSearch] = useState('')
  const [filterBlocked, setFilterBlocked] = useState('')
  const [filterLabel, setFilterLabel] = useState('')
  const [selected, setSelected] = useState<TelegramUser | null>(null)
  const [notes, setNotes] = useState('')
  const [labels, setLabels] = useState('')
  const [exporting, setExporting] = useState(false)

  const botsQuery = useQuery({ queryKey: ['bots'], queryFn: fetchBots })
  const usersQuery = useQuery({
    queryKey: ['users', filterBot, search, filterBlocked, filterLabel],
    queryFn: () =>
      fetchUsers({
        bot_id: filterBot || undefined,
        search: search || undefined,
        blocked: filterBlocked === '' ? undefined : filterBlocked === 'true',
        label: filterLabel || undefined,
      }),
  })
  const detailQuery = useQuery({
    queryKey: ['user', selected?.id],
    queryFn: () => fetchUser(selected!.id),
    enabled: selected !== null,
  })

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['users'] })

  const saveMutation = useMutation({
    mutationFn: () =>
      updateUser(selected!.id, {
        notes: notes.trim(),
        labels: labels.split(',').map((l) => l.trim()).filter(Boolean),
      }),
    onSuccess: () => {
      pushToast('Catatan dan label disimpan.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['user', selected?.id] })
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const blockMutation = useMutation({
    mutationFn: (u: TelegramUser) => (u.is_blocked ? unblockUser(u.id) : blockUser(u.id)),
    onSuccess: (updated) => {
      pushToast(updated.is_blocked ? 'Pengguna diblokir.' : 'Blokir pengguna dibuka.', 'success')
      setSelected(updated)
      void queryClient.invalidateQueries({ queryKey: ['user', updated.id] })
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const openDetail = (u: TelegramUser) => {
    setSelected(u)
    setNotes(u.notes ?? '')
    setLabels((u.labels ?? []).join(', '))
  }

  const handleExport = async () => {
    setExporting(true)
    try {
      await exportUsersCsv(filterBot || undefined)
      pushToast('File CSV pengguna diunduh.', 'success')
    } catch (e) {
      pushToast((e as Error).message, 'error')
    } finally {
      setExporting(false)
    }
  }

  const bots = botsQuery.data ?? []
  const users = usersQuery.data ?? []
  const detail = detailQuery.data ?? selected

  if (usersQuery.isLoading) return <LoadingSpinner label="Memuat daftar pengguna…" />
  if (usersQuery.isError) {
    return (
      <div>
        <PageHeader title="Users" subtitle="Pengguna bot Telegram Anda" />
        <ErrorState message={(usersQuery.error as Error).message} onRetry={() => void usersQuery.refetch()} />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <PageHeader
        title="Users"
        subtitle="Pengguna bot Telegram Anda"
        action={
          <button
            type="button"
            onClick={handleExport}
            disabled={exporting}
            className="flex items-center gap-1 rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white disabled:opacity-50"
          >
            <Download className="h-4 w-4" /> Export CSV
          </button>
        }
      />

      <div className="flex flex-wrap gap-2">
        <div className="relative min-w-0 flex-1">
          <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink/40" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Cari nama / username…"
            className={`${inputCls} pl-9`}
          />
        </div>
        <select value={filterBot} onChange={(e) => setFilterBot(e.target.value)} className={`${inputCls} !w-auto`} aria-label="Filter bot">
          <option value="">Semua bot</option>
          {bots.map((b) => (
            <option key={String(b.id)} value={String(b.id)}>
              {b.name ?? b.username ?? `Bot ${b.id}`}
            </option>
          ))}
        </select>
        <select value={filterBlocked} onChange={(e) => setFilterBlocked(e.target.value)} className={`${inputCls} !w-auto`} aria-label="Filter blokir">
          <option value="">Semua status</option>
          <option value="false">Tidak diblokir</option>
          <option value="true">Diblokir</option>
        </select>
        <input
          value={filterLabel}
          onChange={(e) => setFilterLabel(e.target.value)}
          placeholder="Filter label"
          className={`${inputCls} !w-32`}
        />
      </div>

      {users.length === 0 ? (
        <EmptyState
          icon={<User className="h-10 w-10 text-primary" />}
          title="Belum ada pengguna"
          description="Pengguna yang berinteraksi dengan bot Anda akan muncul di sini."
        />
      ) : (
        users.map((u) => (
          <Card key={String(u.id)}>
            <button type="button" onClick={() => openDetail(u)} className="w-full text-left">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="truncate font-bold">
                    {[u.first_name, u.last_name].filter(Boolean).join(' ') || u.username || `User ${u.telegram_id ?? u.telegram_user_id ?? u.id}`}
                  </p>
                  <p className="text-sm text-ink/60 dark:text-darktext/60">
                    {u.username ? `@${u.username}` : ''}{' '}
                    {u.telegram_id ?? u.telegram_user_id
                      ? `· ID ${u.telegram_id ?? u.telegram_user_id}`
                      : ''}
                  </p>
                  <div className="mt-1.5 flex flex-wrap gap-1.5">
                    {u.is_blocked && <Badge tone="danger">Diblokir</Badge>}
                    {(u.labels ?? []).map((l) => (
                      <Badge key={l} tone="info">{l}</Badge>
                    ))}
                  </div>
                </div>
                <span className="shrink-0 text-xs text-ink/50 dark:text-darktext/50">
                  {u.last_seen ?? u.last_seen_at
                    ? `Terakhir: ${new Date((u.last_seen ?? u.last_seen_at) as string).toLocaleDateString('id-ID')}`
                    : ''}
                </span>
              </div>
            </button>
          </Card>
        ))
      )}

      <Modal open={selected !== null} onClose={() => setSelected(null)} title="Detail Pengguna">
        {detailQuery.isLoading ? (
          <LoadingSpinner label="Memuat detail…" />
        ) : detail ? (
          <div className="space-y-3">
            <div className="text-sm">
              <p className="font-bold">
                {[detail.first_name, detail.last_name].filter(Boolean).join(' ') || detail.username || `User ${detail.telegram_id ?? detail.telegram_user_id ?? detail.id}`}
              </p>
              <p className="text-ink/60 dark:text-darktext/60">
                {detail.username ? `@${detail.username} · ` : ''}Telegram ID: {detail.telegram_id ?? detail.telegram_user_id ?? '—'}
              </p>
              <p className="text-ink/60 dark:text-darktext/60">
                Bergabung: {detail.created_at ? new Date(detail.created_at).toLocaleString('id-ID') : '—'}
              </p>
            </div>
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Catatan</span>
              <textarea value={notes} onChange={(e) => setNotes(e.target.value)} rows={3} placeholder="Catatan internal tentang pengguna ini" className={inputCls} />
            </label>
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Label (pisahkan dengan koma)</span>
              <input value={labels} onChange={(e) => setLabels(e.target.value)} placeholder="mis. vip, pelanggan" className={inputCls} />
            </label>
            <button
              type="button"
              disabled={saveMutation.isPending}
              onClick={() => saveMutation.mutate()}
              className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
            >
              {saveMutation.isPending ? 'Menyimpan…' : 'Simpan Catatan & Label'}
            </button>
            <button
              type="button"
              disabled={blockMutation.isPending}
              onClick={() => blockMutation.mutate(detail)}
              className={`flex w-full items-center justify-center gap-2 rounded-full py-2.5 text-sm font-bold ${
                detail.is_blocked ? 'bg-success/15 text-success' : 'bg-danger/15 text-danger'
              } disabled:opacity-50`}
            >
              <Ban className="h-4 w-4" />
              {detail.is_blocked ? 'Buka Blokir' : 'Blokir Pengguna'}
            </button>
          </div>
        ) : null}
      </Modal>
    </div>
  )
}
