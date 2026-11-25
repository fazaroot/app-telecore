import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, Shield, UsersRound } from 'lucide-react'
import { fetchBots } from '../api/bots'
import { fetchGroup, fetchGroups, fetchModerationLogs } from '../api/users'
import {
  fetchGroupSettings,
  fetchGroupStats,
  moderateGroupMember,
  updateGroupSettings,
  type ModerationAction,
} from '../api/groups'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { Modal } from '../components/Modal'
import type { Group, GroupSettings } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

const VIOLATION_ACTIONS = [
  { value: 'delete', label: 'Hapus pesan' },
  { value: 'warn', label: 'Beri peringatan' },
  { value: 'mute', label: 'Mute pengguna' },
] as const

export function GroupsPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [filterBot, setFilterBot] = useState('')
  const [selected, setSelected] = useState<Group | null>(null)
  const [settings, setSettings] = useState<GroupSettings>({})
  const [bannedWordInput, setBannedWordInput] = useState('')
  const [modUserId, setModUserId] = useState('')
  const [modAction, setModAction] = useState<ModerationAction>('mute')
  const [modDuration, setModDuration] = useState('60')
  const [modReason, setModReason] = useState('')

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
  const settingsQuery = useQuery({
    queryKey: ['group-settings', selected?.id],
    queryFn: () => fetchGroupSettings(selected!.id),
    enabled: selected !== null,
  })
  const statsQuery = useQuery({
    queryKey: ['group-stats', selected?.id],
    queryFn: () => fetchGroupStats(selected!.id),
    enabled: selected !== null,
  })
  const logsQuery = useQuery({
    queryKey: ['moderation-logs', selected?.id],
    queryFn: () => fetchModerationLogs(selected!.id),
    enabled: selected !== null,
  })

  // Sinkronkan pengaturan dari server ke state lokal saat query selesai,
  // agar edit lokal tidak ditimpa data query pada render berikutnya.
  const settingsData = settingsQuery.data
  useEffect(() => {
    if (settingsData !== undefined) setSettings(settingsData)
  }, [settingsData])

  const saveMutation = useMutation({
    mutationFn: () => updateGroupSettings(selected!.id, settings),
    onSuccess: () => {
      pushToast('Pengaturan grup disimpan.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['group-settings', selected?.id] })
      void queryClient.invalidateQueries({ queryKey: ['group', selected?.id] })
      void queryClient.invalidateQueries({ queryKey: ['groups'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const moderateMutation = useMutation({
    mutationFn: () =>
      moderateGroupMember(selected!.id, {
        user_id: modUserId.trim(),
        action: modAction,
        duration_minutes: modAction === 'mute' || modAction === 'ban' ? Number(modDuration) || undefined : undefined,
        reason: modReason.trim() || undefined,
      }),
    onSuccess: () => {
      pushToast('Aksi moderasi dijalankan.', 'success')
      setModUserId('')
      setModReason('')
      void queryClient.invalidateQueries({ queryKey: ['moderation-logs', selected?.id] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const openDetail = (g: Group) => {
    setSelected(g)
    // Pengaturan dari daftar dulu; query settings akan menimpanya saat selesai.
    setSettings(g.settings ?? {})
    setBannedWordInput('')
    setModUserId('')
    setModReason('')
    setModAction('mute')
  }

  const bots = botsQuery.data ?? []
  const groups = groupsQuery.data ?? []
  const detail = detailQuery.data ?? selected
  const logs = logsQuery.data ?? []
  const stats = statsQuery.data
  // Pengaturan efektif = state lokal (sudah disinkron dari query settings).
  const effectiveSettings: GroupSettings = settings
  const botIsAdmin = detail?.bot_is_admin ?? detail?.is_bot_admin
  const bannedWords = effectiveSettings.banned_words ?? effectiveSettings.keyword_filter ?? []

  const set = (patch: Partial<GroupSettings>) => setSettings((s) => ({ ...s, ...patch }))
  const toggle = (key: 'welcome_enabled' | 'anti_spam_enabled' | 'block_links') =>
    setSettings((s) => ({ ...s, [key]: !s[key] }))

  const addBannedWord = () => {
    const w = bannedWordInput.trim()
    if (!w) return
    const next = [...bannedWords, w]
    set({ banned_words: next, keyword_filter: next })
    setBannedWordInput('')
  }
  const removeBannedWord = (word: string) => {
    const next = bannedWords.filter((w) => w !== word)
    set({ banned_words: next, keyword_filter: next })
  }

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
      <PageHeader title="Groups" subtitle="Grup Telegram yang dikelola bot — gratis" />
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

            {/* Statistik */}
            {statsQuery.isLoading ? (
              <LoadingSpinner label="Memuat statistik…" />
            ) : stats ? (
              <div className="grid grid-cols-2 gap-2">
                {[
                  ['Anggota', stats.member_count ?? detail.member_count ?? '—'],
                  ['Pesan hari ini', stats.messages_today ?? '—'],
                  ['Pesan 7 hari', stats.messages_7d ?? '—'],
                  ['User aktif 7 hari', stats.active_users_7d ?? '—'],
                ].map(([label, value]) => (
                  <div key={label} className="rounded-xl border border-ink/10 p-3 dark:border-darktext/10">
                    <p className="text-xs text-ink/60 dark:text-darktext/60">{label}</p>
                    <p className="text-lg font-bold">{value}</p>
                  </div>
                ))}
              </div>
            ) : null}
            {stats?.top_senders && stats.top_senders.length > 0 && (
              <div>
                <h3 className="mb-2 font-bold">Pengirim Teratas (7 hari)</h3>
                <ul className="space-y-1.5">
                  {stats.top_senders.map((s) => (
                    <li key={String(s.user_id)} className="flex items-center justify-between rounded-xl border border-ink/10 px-3 py-2 text-sm dark:border-darktext/10">
                      <span className="font-medium">
                        {s.first_name ?? s.username ?? `User ${s.user_id}`}
                        {s.username ? ` (@${s.username})` : ''}
                      </span>
                      <span className="text-ink/60 dark:text-darktext/60">{s.count} pesan</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}

            {/* Pengaturan grup (kontrak v1.3) */}
            <div className="space-y-3">
              <h3 className="font-bold">Pengaturan Grup</h3>
              <label className="flex items-center justify-between gap-3 text-sm font-medium">
                Pesan sambutan (welcome)
                <input
                  type="checkbox"
                  checked={Boolean(effectiveSettings.welcome_enabled)}
                  onChange={() => toggle('welcome_enabled')}
                  className="h-5 w-5 accent-primary"
                />
              </label>
              <label className="block text-sm">
                <span className="mb-1 block font-medium">Pesan sambutan</span>
                <textarea
                  value={effectiveSettings.welcome_message ?? ''}
                  onChange={(e) => set({ welcome_message: e.target.value })}
                  rows={2}
                  placeholder="Selamat datang {{first_name}} di {{chat_title}}!"
                  className={inputCls}
                />
                <span className="mt-1 block text-xs text-ink/50 dark:text-darktext/50">
                  Template: {'{{first_name}}'} nama anggota baru, {'{{chat_title}}'} nama grup.
                </span>
              </label>

              <label className="flex items-center justify-between gap-3 text-sm font-medium">
                Anti-spam
                <input
                  type="checkbox"
                  checked={Boolean(effectiveSettings.anti_spam_enabled ?? effectiveSettings.anti_spam)}
                  onChange={() => toggle('anti_spam_enabled')}
                  className="h-5 w-5 accent-primary"
                />
              </label>
              <div className="grid grid-cols-2 gap-2">
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Batas flood (pesan)</span>
                  <input
                    type="number"
                    min={1}
                    value={effectiveSettings.flood_limit ?? ''}
                    onChange={(e) => set({ flood_limit: Math.max(1, Number(e.target.value) || 5) })}
                    placeholder="mis. 5"
                    className={inputCls}
                  />
                </label>
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Window flood (detik)</span>
                  <input
                    type="number"
                    min={1}
                    value={effectiveSettings.flood_window_seconds ?? ''}
                    onChange={(e) => set({ flood_window_seconds: Math.max(1, Number(e.target.value) || 10) })}
                    placeholder="mis. 10"
                    className={inputCls}
                  />
                </label>
              </div>
              <label className="flex items-center justify-between gap-3 text-sm font-medium">
                Blokir tautan (block links)
                <input
                  type="checkbox"
                  checked={Boolean(effectiveSettings.block_links ?? effectiveSettings.link_filter)}
                  onChange={() => toggle('block_links')}
                  className="h-5 w-5 accent-primary"
                />
              </label>

              <div>
                <span className="mb-1 block text-sm font-medium">Kata terlarang</span>
                <div className="flex flex-wrap gap-1.5">
                  {bannedWords.length === 0 && (
                    <p className="text-sm text-ink/60 dark:text-darktext/60">Belum ada kata terlarang.</p>
                  )}
                  {bannedWords.map((w) => (
                    <span key={w} className="flex items-center gap-1 rounded-full bg-danger/10 px-2 py-0.5 text-xs font-semibold text-danger">
                      {w}
                      <button type="button" aria-label={`Hapus kata ${w}`} onClick={() => removeBannedWord(w)} className="font-bold leading-none">
                        ×
                      </button>
                    </span>
                  ))}
                </div>
                <div className="mt-2 flex gap-2">
                  <input
                    value={bannedWordInput}
                    onChange={(e) => setBannedWordInput(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') {
                        e.preventDefault()
                        addBannedWord()
                      }
                    }}
                    placeholder="Tambah kata terlarang…"
                    className={`${inputCls} flex-1`}
                  />
                  <button
                    type="button"
                    onClick={addBannedWord}
                    disabled={bannedWordInput.trim() === ''}
                    className="shrink-0 rounded-full bg-primary px-3 py-2 text-sm font-semibold text-white disabled:opacity-50"
                  >
                    Tambah
                  </button>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-2">
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Aksi pelanggaran</span>
                  <select
                    value={effectiveSettings.violation_action ?? 'delete'}
                    onChange={(e) => set({ violation_action: e.target.value })}
                    className={inputCls}
                  >
                    {VIOLATION_ACTIONS.map((a) => (
                      <option key={a.value} value={a.value}>
                        {a.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Durasi mute (menit)</span>
                  <input
                    type="number"
                    min={1}
                    value={effectiveSettings.mute_duration_minutes ?? ''}
                    onChange={(e) => set({ mute_duration_minutes: Math.max(1, Number(e.target.value) || 60) })}
                    placeholder="mis. 60"
                    className={inputCls}
                  />
                </label>
              </div>

              <button
                type="button"
                disabled={saveMutation.isPending}
                onClick={() => saveMutation.mutate()}
                className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
              >
                {saveMutation.isPending ? 'Menyimpan…' : 'Simpan Pengaturan'}
              </button>
            </div>

            {/* Aksi moderasi manual */}
            <div className="space-y-2 border-t border-ink/10 pt-3 dark:border-darktext/10">
              <h3 className="flex items-center gap-1.5 font-bold">
                <Shield className="h-4 w-4" /> Moderasi Anggota
              </h3>
              <input
                value={modUserId}
                onChange={(e) => setModUserId(e.target.value)}
                placeholder="User ID Telegram (angka)"
                inputMode="numeric"
                className={inputCls}
              />
              <div className="grid grid-cols-2 gap-2">
                <select value={modAction} onChange={(e) => setModAction(e.target.value as ModerationAction)} className={inputCls} aria-label="Aksi moderasi">
                  <option value="mute">Mute</option>
                  <option value="unmute">Unmute</option>
                  <option value="ban">Ban</option>
                  <option value="unban">Unban</option>
                </select>
                {(modAction === 'mute' || modAction === 'ban') && (
                  <input
                    type="number"
                    min={1}
                    value={modDuration}
                    onChange={(e) => setModDuration(e.target.value)}
                    placeholder="Durasi (menit)"
                    className={inputCls}
                    aria-label="Durasi (menit)"
                  />
                )}
              </div>
              <input
                value={modReason}
                onChange={(e) => setModReason(e.target.value)}
                placeholder="Alasan (opsional)"
                className={inputCls}
              />
              <button
                type="button"
                disabled={moderateMutation.isPending || modUserId.trim() === ''}
                onClick={() => moderateMutation.mutate()}
                className="w-full rounded-full bg-danger py-2.5 text-sm font-bold text-white disabled:opacity-50"
              >
                {moderateMutation.isPending ? 'Memproses…' : 'Jalankan Aksi'}
              </button>
            </div>

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
