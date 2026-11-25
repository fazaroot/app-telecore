import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Ban, Download, Search, Tag, Trash2, User } from 'lucide-react'
import { fetchBots } from '../api/bots'
import { blockUser, exportUsersCsv, fetchUser, fetchUsers, unblockUser, updateUser } from '../api/users'
import {
  TAG_COLORS,
  addUserNote,
  attachUserTag,
  createTag,
  deleteTag,
  deleteUserNote,
  detachUserTag,
  fetchTags,
  fetchUserNotes,
  fetchUserTags,
  fetchUserTimeline,
} from '../api/tags'
import { fetchSegments } from '../api/segments'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { Modal } from '../components/Modal'
import type { TelegramUser, UserTag } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

const TIMELINE_LABEL: Record<string, string> = {
  message: 'Pesan',
  order: 'Order',
  note: 'Catatan',
  tag: 'Tag',
}

/**
 * Nama tampilan pengguna. Tidak boleh menghasilkan "undefined":
 * nama lengkap → username → "Pengguna <id telegram>" → "Pengguna tanpa nama".
 */
export function userDisplayName(u: TelegramUser): string {
  const fullName = [u.first_name, u.last_name].filter(Boolean).join(' ').trim()
  if (fullName) return fullName
  if (u.username) return u.username
  const telegramId = u.telegram_user_id ?? u.telegram_id
  if (telegramId !== undefined && telegramId !== null && telegramId !== '') {
    return `Pengguna ${telegramId}`
  }
  return 'Pengguna tanpa nama'
}

export function UsersPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [filterBot, setFilterBot] = useState('')
  const [search, setSearch] = useState('')
  const [filterBlocked, setFilterBlocked] = useState('')
  const [filterLabel, setFilterLabel] = useState('')
  const [filterTag, setFilterTag] = useState('')
  const [filterSegment, setFilterSegment] = useState('')
  const [selected, setSelected] = useState<TelegramUser | null>(null)
  const [notes, setNotes] = useState('')
  const [labels, setLabels] = useState('')
  const [exporting, setExporting] = useState(false)
  const [newTagName, setNewTagName] = useState('')
  const [newTagColor, setNewTagColor] = useState(TAG_COLORS[0])
  const [newNote, setNewNote] = useState('')

  const botsQuery = useQuery({ queryKey: ['bots'], queryFn: fetchBots })
  const tagsQuery = useQuery({ queryKey: ['tags'], queryFn: fetchTags })
  const segmentsQuery = useQuery({ queryKey: ['segments'], queryFn: fetchSegments })
  const usersQuery = useQuery({
    queryKey: ['users', filterBot, search, filterBlocked, filterLabel, filterTag, filterSegment],
    queryFn: () =>
      fetchUsers({
        bot_id: filterBot || undefined,
        search: search || undefined,
        blocked: filterBlocked === '' ? undefined : filterBlocked === 'true',
        label: filterLabel || undefined,
        tag_id: filterTag || undefined,
        segment_id: filterSegment || undefined,
      }),
  })
  const detailQuery = useQuery({
    queryKey: ['user', selected?.id],
    queryFn: () => fetchUser(selected!.id),
    enabled: selected !== null,
  })
  const userTagsQuery = useQuery({
    queryKey: ['user-tags', selected?.id],
    queryFn: () => fetchUserTags(selected!.id),
    enabled: selected !== null,
  })
  const userNotesQuery = useQuery({
    queryKey: ['user-notes', selected?.id],
    queryFn: () => fetchUserNotes(selected!.id),
    enabled: selected !== null,
  })
  const timelineQuery = useQuery({
    queryKey: ['user-timeline', selected?.id],
    queryFn: () => fetchUserTimeline(selected!.id),
    enabled: selected !== null,
  })

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['users'] })
  const invalidateUser = () => {
    void queryClient.invalidateQueries({ queryKey: ['user-tags', selected?.id] })
    void queryClient.invalidateQueries({ queryKey: ['user-notes', selected?.id] })
    void queryClient.invalidateQueries({ queryKey: ['user-timeline', selected?.id] })
    void queryClient.invalidateQueries({ queryKey: ['user', selected?.id] })
    invalidate()
  }

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

  const createTagMutation = useMutation({
    mutationFn: () => createTag({ name: newTagName.trim(), color: newTagColor }),
    onSuccess: () => {
      pushToast('Tag dibuat.', 'success')
      setNewTagName('')
      void queryClient.invalidateQueries({ queryKey: ['tags'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteTagMutation = useMutation({
    mutationFn: (tag: UserTag) => deleteTag(tag.id),
    onSuccess: () => {
      pushToast('Tag dihapus.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['tags'] })
      invalidateUser()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const attachTagMutation = useMutation({
    mutationFn: (tag: UserTag) => attachUserTag(selected!.id, tag.id),
    onSuccess: () => {
      pushToast('Tag dipasang.', 'success')
      invalidateUser()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const detachTagMutation = useMutation({
    mutationFn: (tag: UserTag) => detachUserTag(selected!.id, tag.id),
    onSuccess: () => {
      pushToast('Tag dilepas.', 'success')
      invalidateUser()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const addNoteMutation = useMutation({
    mutationFn: () => addUserNote(selected!.id, newNote.trim()),
    onSuccess: () => {
      pushToast('Catatan ditambahkan.', 'success')
      setNewNote('')
      invalidateUser()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteNoteMutation = useMutation({
    mutationFn: (noteId: string | number) => deleteUserNote(selected!.id, noteId),
    onSuccess: () => {
      pushToast('Catatan dihapus.', 'success')
      invalidateUser()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const openDetail = (u: TelegramUser) => {
    setSelected(u)
    setNotes(u.notes ?? '')
    setLabels((u.labels ?? []).join(', '))
    setNewNote('')
  }

  const handleExport = async () => {
    setExporting(true)
    try {
      await exportUsersCsv(filterBot || undefined)
      pushToast('File CSV pengguna diunduh (termasuk kolom tag).', 'success')
    } catch (e) {
      pushToast((e as Error).message, 'error')
    } finally {
      setExporting(false)
    }
  }

  const bots = botsQuery.data ?? []
  const tags = tagsQuery.data ?? []
  const segments = segmentsQuery.data ?? []
  const users = usersQuery.data ?? []
  const detail = detailQuery.data ?? selected
  const userTags = userTagsQuery.data ?? detail?.tags ?? []
  const userNotes = userNotesQuery.data ?? []
  const timeline = timelineQuery.data ?? []
  const attachedTagIds = new Set(userTags.map((t) => String(t.id)))
  const unattachedTags = tags.filter((t) => !attachedTagIds.has(String(t.id)))

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
        subtitle="Pengguna bot Telegram Anda — CRM gratis"
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
        <select value={filterTag} onChange={(e) => setFilterTag(e.target.value)} className={`${inputCls} !w-auto`} aria-label="Filter tag">
          <option value="">Semua tag</option>
          {tags.map((t) => (
            <option key={String(t.id)} value={String(t.id)}>
              {t.name}
            </option>
          ))}
        </select>
        <select value={filterSegment} onChange={(e) => setFilterSegment(e.target.value)} className={`${inputCls} !w-auto`} aria-label="Filter segmen">
          <option value="">Semua segmen</option>
          {segments.map((s) => (
            <option key={String(s.id)} value={String(s.id)}>
              {s.name}
            </option>
          ))}
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
                  <p className="truncate font-bold">{userDisplayName(u)}</p>
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
                    {(u.tags ?? []).map((t) => (
                      <span
                        key={String(t.id)}
                        className="rounded-full px-2 py-0.5 text-xs font-semibold text-white"
                        style={{ backgroundColor: t.color ?? '#526FE5' }}
                      >
                        {t.name}
                      </span>
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
          <div className="space-y-4">
            <div className="text-sm">
              <p className="min-w-0 break-words font-bold [overflow-wrap:anywhere]">{userDisplayName(detail)}</p>
              <p className="min-w-0 break-words text-ink/60 [overflow-wrap:anywhere] dark:text-darktext/60">
                {detail.username ? `@${detail.username} · ` : ''}Telegram ID:{' '}
                {detail.telegram_user_id ?? detail.telegram_id ?? '—'}
              </p>
              <p className="text-ink/60 dark:text-darktext/60">
                Bergabung: {detail.created_at ? new Date(detail.created_at).toLocaleString('id-ID') : '—'}
              </p>
              <p className="text-ink/60 dark:text-darktext/60">
                Terakhir terlihat:{' '}
                {(detail.last_seen_at ?? detail.last_seen)
                  ? new Date((detail.last_seen_at ?? detail.last_seen) as string).toLocaleString('id-ID')
                  : '—'}
              </p>
              <p className="text-ink/60 dark:text-darktext/60">
                Jumlah pesan: {detail.message_count ?? '—'}
              </p>
              <p className="mt-1 flex items-center gap-1.5 text-ink/60 dark:text-darktext/60">
                Status:{' '}
                {detail.is_blocked ? <Badge tone="danger">Diblokir</Badge> : <Badge tone="success">Tidak diblokir</Badge>}
              </p>
            </div>

            {/* Tag pengguna */}
            <div>
              <h3 className="mb-2 flex items-center gap-1.5 font-bold">
                <Tag className="h-4 w-4" /> Tag
              </h3>
              <div className="flex flex-wrap gap-1.5">
                {userTags.length === 0 && (
                  <p className="text-sm text-ink/60 dark:text-darktext/60">Belum ada tag.</p>
                )}
                {userTags.map((t) => (
                  <span
                    key={String(t.id)}
                    className="flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-semibold text-white"
                    style={{ backgroundColor: t.color ?? '#526FE5' }}
                  >
                    {t.name}
                    <button
                      type="button"
                      aria-label={`Lepas tag ${t.name}`}
                      onClick={() => detachTagMutation.mutate(t)}
                      className="font-bold leading-none"
                    >
                      ×
                    </button>
                  </span>
                ))}
              </div>
              {unattachedTags.length > 0 && (
                <select
                  value=""
                  onChange={(e) => {
                    const tag = tags.find((t) => String(t.id) === e.target.value)
                    if (tag) attachTagMutation.mutate(tag)
                  }}
                  className={`${inputCls} mt-2`}
                  aria-label="Pasang tag"
                >
                  <option value="">+ Pasang tag…</option>
                  {unattachedTags.map((t) => (
                    <option key={String(t.id)} value={String(t.id)}>
                      {t.name}
                    </option>
                  ))}
                </select>
              )}
              <div className="mt-2 flex gap-2">
                <input
                  value={newTagName}
                  onChange={(e) => setNewTagName(e.target.value)}
                  placeholder="Nama tag baru"
                  className={`${inputCls} flex-1`}
                />
                <select
                  value={newTagColor}
                  onChange={(e) => setNewTagColor(e.target.value)}
                  className={`${inputCls} !w-auto`}
                  aria-label="Warna tag"
                >
                  {TAG_COLORS.map((c) => (
                    <option key={c} value={c}>
                      {c}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  disabled={createTagMutation.isPending || newTagName.trim() === ''}
                  onClick={() => createTagMutation.mutate()}
                  className="shrink-0 rounded-full bg-primary px-3 py-2 text-sm font-semibold text-white disabled:opacity-50"
                >
                  Buat Tag
                </button>
              </div>
              {tags.length > 0 && (
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {tags.map((t) => (
                    <span key={String(t.id)} className="flex items-center gap-1 rounded-full bg-ink/5 px-2 py-0.5 text-xs dark:bg-darktext/10">
                      <span className="h-2.5 w-2.5 rounded-full" style={{ backgroundColor: t.color ?? '#526FE5' }} />
                      {t.name}
                      <button
                        type="button"
                        aria-label={`Hapus tag ${t.name}`}
                        onClick={() => deleteTagMutation.mutate(t)}
                        className="text-danger"
                      >
                        <Trash2 className="h-3 w-3" />
                      </button>
                    </span>
                  ))}
                </div>
              )}
            </div>

            {/* Catatan (riwayat + tambah) */}
            <div>
              <h3 className="mb-2 font-bold">Catatan</h3>
              {userNotes.length === 0 ? (
                <p className="text-sm text-ink/60 dark:text-darktext/60">Belum ada catatan.</p>
              ) : (
                <ul className="space-y-2">
                  {userNotes.map((n) => (
                    <li key={String(n.id)} className="flex items-start justify-between gap-2 rounded-xl border border-ink/10 p-3 text-sm dark:border-darktext/10">
                      <div className="min-w-0">
                        <p className="break-words [overflow-wrap:anywhere]">{n.note}</p>
                        <p className="mt-0.5 text-xs text-ink/50 dark:text-darktext/50">
                          {n.created_at ? new Date(n.created_at).toLocaleString('id-ID') : ''}
                        </p>
                      </div>
                      <button
                        type="button"
                        aria-label="Hapus catatan"
                        onClick={() => deleteNoteMutation.mutate(n.id)}
                        className="rounded-full bg-danger/10 p-1.5 text-danger"
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <div className="mt-2 flex gap-2">
                <input
                  value={newNote}
                  onChange={(e) => setNewNote(e.target.value)}
                  placeholder="Tulis catatan baru…"
                  className={`${inputCls} flex-1`}
                />
                <button
                  type="button"
                  disabled={addNoteMutation.isPending || newNote.trim() === ''}
                  onClick={() => addNoteMutation.mutate()}
                  className="shrink-0 rounded-full bg-primary px-3 py-2 text-sm font-semibold text-white disabled:opacity-50"
                >
                  Tambah
                </button>
              </div>
              <label className="mt-3 block text-sm">
                <span className="mb-1 block font-medium">Catatan profil (lama)</span>
                <textarea value={notes} onChange={(e) => setNotes(e.target.value)} rows={2} placeholder="Catatan internal tentang pengguna ini" className={inputCls} />
              </label>
              <label className="mt-2 block text-sm">
                <span className="mb-1 block font-medium">Label (pisahkan dengan koma)</span>
                <input value={labels} onChange={(e) => setLabels(e.target.value)} placeholder="mis. vip, pelanggan" className={inputCls} />
              </label>
              <button
                type="button"
                disabled={saveMutation.isPending}
                onClick={() => saveMutation.mutate()}
                className="mt-2 w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
              >
                {saveMutation.isPending ? 'Menyimpan…' : 'Simpan Catatan & Label'}
              </button>
            </div>

            {/* Timeline */}
            <div>
              <h3 className="mb-2 font-bold">Timeline</h3>
              {timelineQuery.isLoading ? (
                <LoadingSpinner label="Memuat timeline…" />
              ) : timeline.length === 0 ? (
                <p className="text-sm text-ink/60 dark:text-darktext/60">Belum ada aktivitas tercatat.</p>
              ) : (
                <ul className="space-y-2">
                  {timeline.map((item, i) => (
                    <li key={`${item.created_at}-${i}`} className="rounded-xl border border-ink/10 p-3 text-sm dark:border-darktext/10">
                      <div className="flex items-center justify-between gap-2">
                        <Badge tone={item.kind === 'order' ? 'success' : item.kind === 'note' ? 'info' : 'default'}>
                          {TIMELINE_LABEL[item.kind] ?? item.kind}
                        </Badge>
                        <span className="text-xs text-ink/50 dark:text-darktext/50">
                          {item.created_at ? new Date(item.created_at).toLocaleString('id-ID') : ''}
                        </span>
                      </div>
                      <p className="mt-1 break-words [overflow-wrap:anywhere]">{item.summary}</p>
                    </li>
                  ))}
                </ul>
              )}
            </div>

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
