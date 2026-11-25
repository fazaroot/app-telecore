import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Bell, CheckCheck, Trash2 } from 'lucide-react'
import {
  deleteNotification,
  fetchNotifications,
  markAllNotificationsRead,
  markNotificationRead,
} from '../api/misc'
import { useUiStore } from '../stores/settings'
import { redactBotTokens } from '../utils/redact'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { ConfirmDialog } from '../components/Modal'
import { useState } from 'react'

export function NotificationsPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [deleting, setDeleting] = useState<string | number | null>(null)

  const query = useQuery({ queryKey: ['notifications'], queryFn: fetchNotifications, refetchInterval: 30_000 })
  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['notifications'] })

  const readMutation = useMutation({
    mutationFn: (id: string | number) => markNotificationRead(id),
    onSuccess: invalidate,
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const readAllMutation = useMutation({
    mutationFn: markAllNotificationsRead,
    onSuccess: () => {
      pushToast('Semua notifikasi ditandai sudah dibaca.', 'success')
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string | number) => deleteNotification(id),
    onSuccess: () => {
      pushToast('Notifikasi dihapus.', 'success')
      setDeleting(null)
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  if (query.isLoading) return <LoadingSpinner label="Memuat notifikasi…" />
  if (query.isError) {
    return (
      <div>
        <PageHeader title="Notifications" subtitle="Pemberitahuan penting dari bot dan sistem" />
        <ErrorState message={(query.error as Error).message} onRetry={() => void query.refetch()} />
      </div>
    )
  }

  const items = query.data ?? []
  const unread = items.filter((n) => !n.is_read).length

  return (
    <div className="space-y-4">
      <PageHeader
        title="Notifications"
        subtitle={unread > 0 ? `${unread} belum dibaca` : 'Pemberitahuan penting dari bot dan sistem'}
        action={
          unread > 0 ? (
            <button
              type="button"
              onClick={() => readAllMutation.mutate()}
              disabled={readAllMutation.isPending}
              className="flex items-center gap-1 rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white disabled:opacity-50"
            >
              <CheckCheck className="h-4 w-4" /> Tandai semua dibaca
            </button>
          ) : undefined
        }
      />

      {items.length === 0 ? (
        <EmptyState
          icon={<Bell className="h-10 w-10 text-primary" />}
          title="Tidak ada notifikasi"
          description="Notifikasi penting (bot terputus, automation gagal, error API) akan muncul di sini."
        />
      ) : (
        items.map((n) => (
          <Card key={String(n.id)} className={n.is_read ? 'opacity-75' : ''}>
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-1.5">
                  <p className="min-w-0 break-words font-bold [overflow-wrap:anywhere]">
                    {redactBotTokens(n.title ?? 'Notifikasi')}
                  </p>
                  {!n.is_read && <Badge tone="info">Baru</Badge>}
                  {n.severity && (
                    <Badge tone={n.severity === 'error' || n.severity === 'critical' ? 'danger' : 'default'}>
                      {n.severity}
                    </Badge>
                  )}
                </div>
                {/* URL/teks panjang tanpa spasi harus wrap, bukan overflow menyamping. */}
                <p className="mt-1 min-w-0 break-words text-sm text-ink/70 [overflow-wrap:anywhere] dark:text-darktext/70">
                  {redactBotTokens(n.message ?? n.body ?? '')}
                </p>
                <p className="mt-1 min-w-0 break-words text-xs text-ink/50 [overflow-wrap:anywhere] dark:text-darktext/50">
                  {n.created_at ? new Date(n.created_at).toLocaleString('id-ID') : ''}
                  {n.type ? ` · ${redactBotTokens(n.type)}` : ''}
                </p>
              </div>
              <div className="flex shrink-0 gap-1">
                {!n.is_read && (
                  <button
                    type="button"
                    aria-label="Tandai dibaca"
                    onClick={() => readMutation.mutate(n.id)}
                    disabled={readMutation.isPending}
                    className="rounded-full p-2 text-primary hover:bg-primary/10"
                  >
                    <CheckCheck className="h-4 w-4" />
                  </button>
                )}
                <button
                  type="button"
                  aria-label="Hapus notifikasi"
                  onClick={() => setDeleting(n.id)}
                  className="rounded-full p-2 text-danger hover:bg-danger/10"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            </div>
          </Card>
        ))
      )}

      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting !== null && deleteMutation.mutate(deleting)}
        title="Hapus Notifikasi"
        message="Hapus notifikasi ini secara permanen?"
        busy={deleteMutation.isPending}
      />
    </div>
  )
}
