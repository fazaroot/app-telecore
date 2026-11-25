import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, UsersRound } from 'lucide-react'
import { fetchBots } from '../api/bots'
import { fetchGroup, fetchGroups, fetchModerationLogs, updateGroupSettings } from '../api/users'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { Modal } from '../components/Modal'
import type { Group, GroupSettings } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

export function GroupsPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [filterBot, setFilterBot] = useState('')
  const [selected, setSelected] = useState<Group | null>(null)
  const [settings, setSettings] = useState<GroupSettings>({})
  const [keywordInput, setKeywordInput] = useState('')

  const botsQuery = useQuery({ queryKey: ['bots'], queryFn: fetchBots })
  const groupsQuery = useQuery({
    queryKey: ['groups', filterBot],
    queryFn: () => fetchGroups(filterBot || undefined),
  })
  const detailQuery = useQuery({
    queryKey: ['group', selected?.id],
    queryFn: () => fetchGroup(selected!.id),
    enabled: selected !== null,
  })
  const logsQuery = useQuery({
    queryKey: ['moderation-logs', selected?.id],
    queryFn: () => fetchModerationLogs(selected!.id),
    enabled: selected !== null,
  })

  const saveMutation = useMutation({
    mutationFn: () => updateGroupSettings(selected!.id, settings),
    onSuccess: () => {
      pushToast('Pengaturan grup disimpan.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['group', selected?.id] })
      void queryClient.invalidateQueries({ queryKey: ['groups'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const openDetail = (g: Group) => {
    setSelected(g)
    setSettings(g.settings ?? {})
    setKeywordInput((g.settings?.keyword_filter ?? []).join(', '))
  }

  const bots = botsQuery.data ?? []
  const groups = groupsQuery.data ?? []
  const detail = detailQuery.data ?? selected
  const logs = logsQuery.data ?? []
  const botIsAdmin = detail?.bot_is_admin ?? detail?.is_bot_admin

  const toggle = (key: keyof GroupSettings) =>
    setSettings((s) => ({ ...s, [key]: !s[key] }))

  if (groupsQuery.isLoading) return <LoadingSpinner label="Memuat daftar grup…" />
  if (groupsQuery.isError) {
    return (
      <div>
        <PageHeader title="Groups" subtitle="Grup Telegram yang dikelola bot" />
        <ErrorState message={(groupsQuery.error as Error).message} onRetry={() => void groupsQuery.refetch()} />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <PageHeader title="Groups" subtitle="Grup Telegram yang dikelola bot" />
      <select
        value={filterBot}
        onChange={(e) => setFilterBot(e.target.value)}
        className={`${inputCls} !w-auto`}
        aria-label="Filter bot"
      >
        <option value="">Semua bot</option>
        {bots.map((b) => (
          <option key={String(b.id)} value={String(b.id)}>
            {b.name ?? b.username ?? `Bot ${b.id}`}
          </option>
        ))}
      </select>

      {groups.length === 0 ? (
        <EmptyState
          icon={<UsersRound className="h-10 w-10 text-primary" />}
          title="Belum ada grup"
          description="Tambahkan bot ke grup Telegram agar grup muncul di sini."
        />
      ) : (
        groups.map((g) => (
          <Card key={String(g.id)}>
            <button type="button" onClick={() => openDetail(g)} className="w-full text-left">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="truncate font-bold">{g.title ?? g.name ?? `Grup ${g.telegram_id ?? g.chat_id ?? g.id}`}</p>
                  <p className="text-sm text-ink/60 dark:text-darktext/60">
                    {g.member_count !== undefined ? `${g.member_count} anggota` : ''}{' '}
                    {g.telegram_id ?? g.chat_id ? `· ID ${g.telegram_id ?? g.chat_id}` : ''}
                  </p>
                </div>
                {(g.bot_is_admin ?? g.is_bot_admin) === false ? (
                  <Badge tone="warning">Bot bukan admin</Badge>
                ) : (g.bot_is_admin ?? g.is_bot_admin) === true ? (
                  <Badge tone="success">Bot admin</Badge>
                ) : null}
              </div>
            </button>
          </Card>
        ))
      )}

      <Modal open={selected !== null} onClose={() => setSelected(null)} title={detail?.title ?? detail?.name ?? 'Detail Grup'}>
        {detailQuery.isLoading ? (
          <LoadingSpinner label="Memuat detail grup…" />
        ) : detail ? (
          <div className="space-y-4">
            {botIsAdmin === false && (
              <div className="flex items-start gap-2 rounded-xl bg-amber-500/15 p-3 text-sm text-amber-700 dark:text-amber-400">
                <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
                <span>
                  Bot bukan admin di grup ini — aksi moderasi (hapus pesan, kick, ban, mute) tidak akan berjalan
                  sampai bot dijadikan admin.
                </span>
              </div>
            )}
            <p className="text-sm text-ink/60 dark:text-darktext/60">
              {detail.member_count !== undefined ? `${detail.member_count} anggota · ` : ''}Telegram ID:{' '}
              {detail.telegram_id ?? detail.chat_id ?? '—'}
            </p>

            <label className="block text-sm">
              <span className="mb-1 block font-medium">Pesan sambutan (welcome message)</span>
              <textarea
                value={settings.welcome_message ?? ''}
                onChange={(e) => setSettings((s) => ({ ...s, welcome_message: e.target.value }))}
                rows={2}
                placeholder="Pesan untuk anggota baru"
                className={inputCls}
              />
            </label>
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Filter kata kunci (pisahkan dengan koma)</span>
              <input
                value={keywordInput}
                onChange={(e) => {
                  setKeywordInput(e.target.value)
                  setSettings((s) => ({
                    ...s,
                    keyword_filter: e.target.value.split(',').map((k) => k.trim()).filter(Boolean),
                  }))
                }}
                placeholder="mis. spam, promo, judi"
                className={inputCls}
              />
            </label>

            <div className="space-y-2">
              {(
                [
                  ['anti_spam', 'Anti-spam'],
                  ['link_filter', 'Filter tautan'],
                  ['flood_detection', 'Deteksi flood (pesan beruntun)'],
                  ['warnings_enabled', 'Sistem peringatan'],
                ] as const
              ).map(([key, label]) => (
                <label key={key} className="flex items-center justify-between gap-3 text-sm font-medium">
                  {label}
                  <input
                    type="checkbox"
                    checked={Boolean(settings[key])}
                    onChange={() => toggle(key)}
                    className="h-5 w-5 accent-primary"
                  />
                </label>
              ))}
              {settings.warnings_enabled && (
                <label className="flex items-center justify-between gap-3 text-sm font-medium">
                  Maksimal peringatan sebelum tindakan
                  <input
                    type="number"
                    min={1}
                    value={settings.max_warnings ?? 3}
                    onChange={(e) =>
                      setSettings((s) => ({ ...s, max_warnings: Math.max(1, Number(e.target.value) || 3) }))
                    }
                    className="w-20 rounded-lg border border-ink/15 bg-bg px-2 py-1.5 text-sm dark:border-darktext/15 dark:bg-darkbg"
                  />
                </label>
              )}
            </div>

            <button
              type="button"
              disabled={saveMutation.isPending}
              onClick={() => saveMutation.mutate()}
              className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
            >
              {saveMutation.isPending ? 'Menyimpan…' : 'Simpan Pengaturan'}
            </button>

            <div>
              <h3 className="mb-2 font-bold">Log Moderasi</h3>
              {logsQuery.isLoading ? (
                <LoadingSpinner label="Memuat log moderasi…" />
              ) : logsQuery.isError ? (
                <ErrorState message={(logsQuery.error as Error).message} onRetry={() => void logsQuery.refetch()} />
              ) : logs.length === 0 ? (
                <EmptyState title="Belum ada log moderasi" description="Tindakan moderasi bot akan tercatat di sini." />
              ) : (
                <ul className="space-y-2">
                  {logs.map((log) => (
                    <li key={String(log.id)} className="rounded-xl border border-ink/10 p-3 text-sm dark:border-darktext/10">
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-semibold">{log.action ?? '—'}</span>
                        <span className="text-xs text-ink/50 dark:text-darktext/50">
                          {log.created_at ? new Date(log.created_at).toLocaleString('id-ID') : ''}
                        </span>
                      </div>
                      {log.reason && <p className="mt-0.5 text-ink/70 dark:text-darktext/70">{log.reason}</p>}
                      {log.user_id !== undefined && <p className="text-xs text-ink/50 dark:text-darktext/50">User ID: {log.user_id}</p>}
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        ) : null}
      </Modal>
    </div>
  )
}
