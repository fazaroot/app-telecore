import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Filter, Send, Trash2, Users } from 'lucide-react'
import { fetchTags } from '../api/tags'
import {
  createSegment,
  criteriaSummary,
  deleteSegment,
  fetchSegmentMembers,
  fetchSegments,
  previewSegment,
  sendSegmentFollowUpAndWait,
  updateSegment,
} from '../api/segments'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { ConfirmDialog, Modal } from '../components/Modal'
import type { Segment, SegmentCriteria, UserTag } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

function SegmentForm({
  initial,
  tags,
  onClose,
}: {
  initial: Segment | null
  tags: UserTag[]
  onClose: () => void
}) {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [name, setName] = useState(initial?.name ?? '')
  const [criteria, setCriteria] = useState<SegmentCriteria>(initial?.criteria ?? {})
  const [previewCount, setPreviewCount] = useState<number | null>(null)
  const [previewLoading, setPreviewLoading] = useState(false)

  // Preview jumlah anggota (debounce 400 ms) setiap kali kriteria berubah.
  useEffect(() => {
    setPreviewLoading(true)
    const id = window.setTimeout(() => {
      previewSegment(criteria)
        .then((r) => setPreviewCount(r.count ?? 0))
        .catch(() => setPreviewCount(null))
        .finally(() => setPreviewLoading(false))
    }, 400)
    return () => window.clearTimeout(id)
  }, [criteria])

  const saveMutation = useMutation({
    mutationFn: () =>
      initial !== null
        ? updateSegment(initial.id, { name: name.trim(), criteria })
        : createSegment({ name: name.trim(), criteria }),
    onSuccess: () => {
      pushToast(initial !== null ? 'Segmen diperbarui.' : 'Segmen dibuat.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['segments'] })
      onClose()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const setNum = (key: 'last_active_within_days' | 'last_active_over_days_ago' | 'min_messages', raw: string) => {
    const v = raw === '' ? undefined : Math.max(0, Number(raw) || 0)
    setCriteria((c) => {
      const next = { ...c }
      if (v === undefined) delete next[key]
      else next[key] = v
      return next
    })
  }

  const toggleTag = (tagId: string | number) => {
    setCriteria((c) => {
      const current = c.any_tags ?? []
      const has = current.some((t) => String(t) === String(tagId))
      return {
        ...c,
        any_tags: has ? current.filter((t) => String(t) !== String(tagId)) : [...current, tagId],
      }
    })
  }

  return (
    <div className="space-y-3">
      <label className="block text-sm">
        <span className="mb-1 block font-medium">Nama segmen</span>
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="mis. Pelanggan aktif" className={inputCls} />
      </label>

      <div>
        <p className="mb-1 text-sm font-medium">Tag (salah satu — OR)</p>
        {tags.length === 0 ? (
          <p className="text-sm text-ink/60 dark:text-darktext/60">Belum ada tag. Buat tag dulu di halaman Users.</p>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {tags.map((t) => {
              const active = (criteria.any_tags ?? []).some((x) => String(x) === String(t.id))
              return (
                <button
                  key={String(t.id)}
                  type="button"
                  onClick={() => toggleTag(t.id)}
                  className={`rounded-full px-3 py-1 text-xs font-semibold ${
                    active ? 'text-white' : 'bg-ink/10 text-ink/70 dark:bg-darktext/10 dark:text-darktext/70'
                  }`}
                  style={active ? { backgroundColor: t.color ?? '#526FE5' } : undefined}
                >
                  {t.name}
                </button>
              )
            })}
          </div>
        )}
      </div>

      <label className="flex items-center justify-between gap-3 text-sm font-medium">
        Sudah pernah order
        <input
          type="checkbox"
          checked={criteria.has_ordered === true}
          onChange={(e) =>
            setCriteria((c) => {
              const next = { ...c }
              if (e.target.checked) next.has_ordered = true
              else delete next.has_ordered
              return next
            })
          }
          className="h-5 w-5 accent-primary"
        />
      </label>

      <div className="grid grid-cols-2 gap-2">
        <label className="block text-sm">
          <span className="mb-1 block font-medium">Aktif ≤ N hari terakhir</span>
          <input
            type="number"
            min={0}
            value={criteria.last_active_within_days ?? ''}
            onChange={(e) => setNum('last_active_within_days', e.target.value)}
            placeholder="mis. 7"
            className={inputCls}
          />
        </label>
        <label className="block text-sm">
          <span className="mb-1 block font-medium">Tidak aktif &gt; N hari</span>
          <input
            type="number"
            min={0}
            value={criteria.last_active_over_days_ago ?? ''}
            onChange={(e) => setNum('last_active_over_days_ago', e.target.value)}
            placeholder="mis. 30"
            className={inputCls}
          />
        </label>
      </div>
      <label className="block text-sm">
        <span className="mb-1 block font-medium">Minimal jumlah pesan</span>
        <input
          type="number"
          min={0}
          value={criteria.min_messages ?? ''}
          onChange={(e) => setNum('min_messages', e.target.value)}
          placeholder="mis. 10"
          className={inputCls}
        />
      </label>

      <p className="rounded-xl bg-primary/10 p-3 text-sm font-medium text-primary" aria-live="polite">
        {previewLoading
          ? 'Menghitung preview…'
          : previewCount !== null
            ? `Preview: ${previewCount} anggota cocok dengan kriteria ini.`
            : 'Preview belum tersedia — periksa koneksi backend.'}
      </p>

      <button
        type="button"
        disabled={saveMutation.isPending || name.trim() === ''}
        onClick={() => saveMutation.mutate()}
        className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
      >
        {saveMutation.isPending ? 'Menyimpan…' : initial !== null ? 'Simpan Perubahan' : 'Buat Segmen'}
      </button>
    </div>
  )
}

export function SegmentsPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [formOpen, setFormOpen] = useState(false)
  const [editing, setEditing] = useState<Segment | null>(null)
  const [deleting, setDeleting] = useState<Segment | null>(null)
  const [viewing, setViewing] = useState<Segment | null>(null)
  const [followUpText, setFollowUpText] = useState('')
  const [followUpResult, setFollowUpResult] = useState<
    { sent: number; failed: number; total: number; capped?: boolean; matched?: number; pending?: boolean } | null
  >(null)

  const tagsQuery = useQuery({ queryKey: ['tags'], queryFn: fetchTags })
  const segmentsQuery = useQuery({ queryKey: ['segments'], queryFn: fetchSegments })
  const membersQuery = useQuery({
    queryKey: ['segment-members', viewing?.id],
    queryFn: () => fetchSegmentMembers(viewing!.id),
    enabled: viewing !== null,
  })

  const deleteMutation = useMutation({
    mutationFn: () => deleteSegment(deleting!.id),
    onSuccess: () => {
      pushToast('Segmen dihapus.', 'success')
      setDeleting(null)
      void queryClient.invalidateQueries({ queryKey: ['segments'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  // Follow-up menunggu sampai broadcast selesai (polling): angka snapshot
  // awal backend (sent 0, status running) TIDAK BOLEH ditampilkan sebagai
  // hasil akhir — lihat sendSegmentFollowUpAndWait di api/segments.
  const followUpMutation = useMutation({
    mutationFn: () => sendSegmentFollowUpAndWait(viewing!.id, { message: followUpText.trim() }),
    onSuccess: (r) => {
      const pending = r.status === 'running'
      setFollowUpResult({
        sent: r.sent,
        failed: r.failed,
        total: r.total,
        capped: r.capped_at_1000 === true,
        matched: r.matched,
        pending,
      })
      pushToast(
        pending
          ? `Follow-up masih dikirim (terkirim sejauh ini ${r.sent} dari ${r.total}).`
          : `Follow-up terkirim ke ${r.sent} dari ${r.total} anggota${r.failed > 0 ? `, ${r.failed} gagal` : ''}.`,
        pending ? 'info' : 'success',
      )
      setFollowUpText('')
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const openCreate = () => {
    setEditing(null)
    setFormOpen(true)
  }
  const openEdit = (s: Segment) => {
    setEditing(s)
    setFormOpen(true)
  }
  const openMembers = (s: Segment) => {
    setViewing(s)
    setFollowUpResult(null)
    setFollowUpText('')
  }

  const tags = tagsQuery.data ?? []
  const segments = segmentsQuery.data ?? []
  const tagName = (id: string | number) => tags.find((t) => String(t.id) === String(id))?.name ?? String(id)
  const members = membersQuery.data

  if (segmentsQuery.isLoading) return <LoadingSpinner label="Memuat segmen…" />
  if (segmentsQuery.isError) {
    return (
      <div>
        <PageHeader title="Segmen" subtitle="Kelompok pengguna berdasarkan kriteria" />
        <ErrorState message={(segmentsQuery.error as Error).message} onRetry={() => void segmentsQuery.refetch()} />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <PageHeader
        title="Segmen"
        subtitle="Kelompok pengguna berdasarkan kriteria — gratis"
        action={
          <button
            type="button"
            onClick={openCreate}
            className="rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
          >
            + Buat Segmen
          </button>
        }
      />

      {segments.length === 0 ? (
        <EmptyState
          icon={<Filter className="h-10 w-10 text-primary" />}
          title="Belum ada segmen"
          description="Buat segmen dari tag, riwayat order, dan aktivitas pengguna untuk follow-up tertarget."
        />
      ) : (
        segments.map((s) => (
          <Card key={String(s.id)}>
            <div className="flex items-start justify-between gap-3">
              <button type="button" onClick={() => openMembers(s)} className="min-w-0 flex-1 text-left">
                <p className="truncate font-bold">{s.name}</p>
                <p className="text-sm text-ink/60 dark:text-darktext/60">{criteriaSummary(s.criteria, tagName)}</p>
                <div className="mt-1.5 flex flex-wrap gap-1.5">
                  <Badge tone="info">
                    {s.member_count !== undefined ? `${s.member_count} anggota` : 'Lihat anggota'}
                  </Badge>
                </div>
              </button>
              <div className="flex shrink-0 gap-1">
                <button
                  type="button"
                  onClick={() => openEdit(s)}
                  className="rounded-full bg-ink/5 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
                >
                  Edit
                </button>
                <button
                  type="button"
                  onClick={() => setDeleting(s)}
                  aria-label={`Hapus segmen ${s.name}`}
                  className="rounded-full bg-danger/10 p-1.5 text-danger"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            </div>
          </Card>
        ))
      )}

      {/* Form buat/edit — hanya di-render saat modal terbuka; `editing` boleh null (buat baru). */}
      <Modal open={formOpen} onClose={() => setFormOpen(false)} title={editing !== null ? 'Edit Segmen' : 'Buat Segmen'}>
        {formOpen && <SegmentForm initial={editing} tags={tags} onClose={() => setFormOpen(false)} />}
      </Modal>

      {/* Anggota + follow-up — guard `viewing !== null` sebelum dereferensi apa pun. */}
      <Modal open={viewing !== null} onClose={() => setViewing(null)} title={viewing !== null ? `Anggota: ${viewing.name}` : 'Anggota Segmen'}>
        {viewing !== null && (
          <div className="space-y-4">
            {membersQuery.isLoading ? (
              <LoadingSpinner label="Memuat anggota…" />
            ) : membersQuery.isError ? (
              <ErrorState message={(membersQuery.error as Error).message} onRetry={() => void membersQuery.refetch()} />
            ) : !members || members.items.length === 0 ? (
              <EmptyState
                icon={<Users className="h-10 w-10 text-primary" />}
                title="Belum ada anggota"
                description="Tidak ada pengguna yang cocok dengan kriteria segmen ini."
              />
            ) : (
              <>
                <p className="text-sm text-ink/60 dark:text-darktext/60">
                  {members.total} anggota · menampilkan {members.items.length}
                </p>
                <ul className="space-y-2">
                  {members.items.map((u) => (
                    <li key={String(u.id)} className="rounded-xl border border-ink/10 p-3 text-sm dark:border-darktext/10">
                      <p className="font-semibold">
                        {[u.first_name, u.last_name].filter(Boolean).join(' ') || u.username || `User ${u.telegram_id ?? u.telegram_user_id ?? u.id}`}
                      </p>
                      <p className="text-ink/60 dark:text-darktext/60">
                        {u.username ? `@${u.username}` : ''}{' '}
                        {u.telegram_id ?? u.telegram_user_id ? `· ID ${u.telegram_id ?? u.telegram_user_id}` : ''}
                      </p>
                    </li>
                  ))}
                </ul>
              </>
            )}

            <div className="border-t border-ink/10 pt-3 dark:border-darktext/10">
              <h3 className="mb-2 font-bold">Follow-up ke Segmen</h3>
              <textarea
                value={followUpText}
                onChange={(e) => setFollowUpText(e.target.value)}
                rows={3}
                placeholder="Pesan untuk semua anggota segmen ini…"
                className={inputCls}
              />
              {followUpResult !== null && (
                <p className="mt-2 rounded-xl bg-success/10 p-3 text-sm font-medium text-success">
                  {followUpResult.pending ? 'Masih mengirim — ' : ''}
                  Terkirim: {followUpResult.sent} · Gagal: {followUpResult.failed} · Total: {followUpResult.total}
                  {followUpResult.capped ? ` · Hanya 1000 penerima pertama yang dikirimi (dari ${followUpResult.matched ?? 'lebih dari 1000'} anggota).` : ''}
                </p>
              )}
              <button
                type="button"
                disabled={followUpMutation.isPending || followUpText.trim() === ''}
                onClick={() => followUpMutation.mutate()}
                className="mt-2 flex w-full items-center justify-center gap-2 rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
              >
                <Send className="h-4 w-4" />
                {followUpMutation.isPending ? 'Mengirim…' : 'Kirim ke Semua Anggota'}
              </button>
            </div>
          </div>
        )}
      </Modal>

      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleteMutation.mutate()}
        title="Hapus Segmen"
        message={deleting !== null ? `Hapus segmen "${deleting.name}"? Anggota tidak ikut terhapus.` : ''}
        busy={deleteMutation.isPending}
      />
    </div>
  )
}
