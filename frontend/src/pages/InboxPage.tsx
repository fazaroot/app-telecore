import { useEffect, useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Inbox as InboxIcon, Send } from 'lucide-react'
import { fetchInboxConversations, fetchInboxThread, replyInbox, conversationInitials, conversationTitle, isInbound } from '../api/inbox'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import type { InboxConversation } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

export function InboxPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [selected, setSelected] = useState<InboxConversation | null>(null)
  const [replyText, setReplyText] = useState('')
  const threadEndRef = useRef<HTMLDivElement>(null)

  const conversationsQuery = useQuery({
    queryKey: ['inbox'],
    queryFn: fetchInboxConversations,
    refetchInterval: 15_000,
  })

  // Membuka thread menandai percakapan sebagai dibaca di server.
  const threadQuery = useQuery({
    queryKey: ['inbox-thread', selected?.chat_id],
    queryFn: () => fetchInboxThread(selected!.chat_id),
    enabled: selected !== null,
  })

  useEffect(() => {
    threadEndRef.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [threadQuery.data?.messages?.length])

  const replyMutation = useMutation({
    mutationFn: () => replyInbox(selected!.chat_id, { text: replyText.trim() }),
    onSuccess: () => {
      setReplyText('')
      pushToast('Balasan dikirim.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['inbox-thread', selected?.chat_id] })
      void queryClient.invalidateQueries({ queryKey: ['inbox'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const conversations = conversationsQuery.data ?? []
  const thread = threadQuery.data
  const threadMessages = thread?.messages ?? []

  if (conversationsQuery.isLoading) return <LoadingSpinner label="Memuat inbox…" />
  if (conversationsQuery.isError) {
    return (
      <div>
        <PageHeader title="Inbox" subtitle="Percakapan langsung dengan pengguna bot" />
        <ErrorState
          message={(conversationsQuery.error as Error).message}
          onRetry={() => void conversationsQuery.refetch()}
        />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <PageHeader title="Inbox" subtitle="Percakapan langsung dengan pengguna bot" />

      {selected === null ? (
        conversations.length === 0 ? (
          <EmptyState
            icon={<InboxIcon className="h-10 w-10 text-primary" />}
            title="Belum ada percakapan"
            description="Percakapan muncul setelah ada pengguna yang mengirim pesan ke bot Anda."
          />
        ) : (
          <div className="space-y-2">
            {conversations.map((c) => (
              <button
                key={String(c.chat_id)}
                type="button"
                onClick={() => setSelected(c)}
                className="flex w-full items-center gap-3 rounded-2xl bg-surface p-3 text-left shadow-sm dark:bg-darksurface"
              >
                <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-primary/15 text-sm font-bold text-primary">
                  {conversationInitials(c)}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="flex items-center justify-between gap-2">
                    <span className="truncate font-semibold">{conversationTitle(c)}</span>
                    {c.last_message_at && (
                      <span className="shrink-0 text-xs text-ink/50 dark:text-darktext/50">
                        {new Date(c.last_message_at).toLocaleString('id-ID', {
                          day: '2-digit',
                          month: 'short',
                          hour: '2-digit',
                          minute: '2-digit',
                        })}
                      </span>
                    )}
                  </span>
                  <span className="flex items-center justify-between gap-2">
                    <span className="truncate text-sm text-ink/60 dark:text-darktext/60">
                      {c.last_message_text ?? 'Belum ada pesan'}
                    </span>
                    {(c.unread_count ?? 0) > 0 && (
                      <Badge tone="info">{c.unread_count} baru</Badge>
                    )}
                  </span>
                </span>
              </button>
            ))}
          </div>
        )
      ) : (
        <div className="space-y-3">
          <button
            type="button"
            onClick={() => setSelected(null)}
            className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-sm font-semibold dark:bg-darktext/10"
          >
            <ArrowLeft className="h-4 w-4" /> Semua percakapan
          </button>

          <Card>
            <div className="flex items-center gap-3 border-b border-ink/10 pb-3 dark:border-darktext/10">
              <span className="flex h-10 w-10 items-center justify-center rounded-full bg-primary/15 text-sm font-bold text-primary">
                {conversationInitials(selected)}
              </span>
              <div className="min-w-0">
                <p className="truncate font-bold">{conversationTitle(selected)}</p>
                <p className="text-xs text-ink/50 dark:text-darktext/50">
                  {selected.chat_type ?? 'chat'} · ID {selected.chat_id}
                </p>
              </div>
            </div>

            {threadQuery.isLoading ? (
              <LoadingSpinner label="Memuat percakapan…" />
            ) : threadQuery.isError ? (
              <ErrorState
                message={(threadQuery.error as Error).message}
                onRetry={() => void threadQuery.refetch()}
              />
            ) : threadMessages.length === 0 ? (
              <EmptyState
                title="Belum ada pesan di percakapan ini"
                description="Kirim balasan pertama dari composer di bawah."
              />
            ) : (
              <div className="space-y-2 py-3">
                {threadMessages.map((m, i) => {
                  const inbound = isInbound(m)
                  return (
                    <div key={String(m.id ?? i)} className={`flex ${inbound ? 'justify-start' : 'justify-end'}`}>
                      <div
                        className={`max-w-[85%] rounded-2xl px-3 py-2 text-sm ${
                          inbound
                            ? 'bg-ink/10 dark:bg-darktext/10'
                            : 'bg-primary text-white'
                        }`}
                      >
                        <p className="whitespace-pre-wrap">
                          {m.text ?? m.content ?? (m.message_type ? `(${m.message_type})` : '(pesan tanpa teks)')}
                        </p>
                        {m.created_at && (
                          <p className={`mt-1 text-right text-[11px] ${inbound ? 'text-ink/50 dark:text-darktext/50' : 'text-white/70'}`}>
                            {new Date(m.created_at).toLocaleString('id-ID', {
                              day: '2-digit',
                              month: 'short',
                              hour: '2-digit',
                              minute: '2-digit',
                            })}
                          </p>
                        )}
                      </div>
                    </div>
                  )
                })}
                <div ref={threadEndRef} />
              </div>
            )}
          </Card>

          <Card>
            <div className="flex items-end gap-2">
              <textarea
                value={replyText}
                onChange={(e) => setReplyText(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault()
                    if (replyText.trim() && !replyMutation.isPending) replyMutation.mutate()
                  }
                }}
                placeholder="Tulis balasan… (Enter untuk kirim)"
                rows={2}
                className={inputCls}
              />
              <button
                type="button"
                aria-label="Kirim balasan"
                onClick={() => replyMutation.mutate()}
                disabled={!replyText.trim() || replyMutation.isPending}
                className="flex shrink-0 items-center gap-1 rounded-full bg-primary px-4 py-2.5 text-sm font-bold text-white disabled:opacity-50"
              >
                <Send className="h-4 w-4" />
                {replyMutation.isPending ? 'Mengirim…' : 'Kirim'}
              </button>
            </div>
            <p className="mt-2 text-xs text-ink/50 dark:text-darktext/50">
              Balasan dikirim sebagai pesan bot ke chat ini. Pesan yang sudah dibaca tidak lagi
              dihitung sebagai belum dibaca.
            </p>
          </Card>
        </div>
      )}
    </div>
  )
}
