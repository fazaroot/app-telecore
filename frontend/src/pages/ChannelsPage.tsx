import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CalendarClock, Megaphone, Send } from 'lucide-react'
import { datetimeLocalToIso, fetchChannels, nowDatetimeLocal, postToChannel, scheduleChannelPost } from '../api/channels'
import { fetchMessages, messageTypeLabel } from '../api/messages'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import type { Channel } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

export function ChannelsPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [selected, setSelected] = useState<Channel | null>(null)
  const [text, setText] = useState('')
  const [parseMode, setParseMode] = useState<'MarkdownV2' | 'HTML' | 'none'>('none')
  const [scheduleAt, setScheduleAt] = useState('')
  const [scheduleMode, setScheduleMode] = useState(false)

  const channelsQuery = useQuery({ queryKey: ['channels'], queryFn: fetchChannels })

  // Riwayat posting channel = pesan keluar terakhir ke chat_id channel itu.
  const historyQuery = useQuery({
    queryKey: ['channel-history', selected?.chat_id],
    queryFn: () => fetchMessages({ chat_id: selected!.chat_id }),
    enabled: selected !== null,
  })

  const postMutation = useMutation({
    mutationFn: () =>
      postToChannel(selected!.chat_id, {
        bot_id: selected!.bot_id ?? '',
        text: text.trim(),
        parse_mode: parseMode,
      }),
    onSuccess: () => {
      pushToast('Postingan dikirim ke channel.', 'success')
      setText('')
      void queryClient.invalidateQueries({ queryKey: ['channel-history', selected?.chat_id] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const scheduleMutation = useMutation({
    mutationFn: () => {
      const runAt = datetimeLocalToIso(scheduleAt)
      if (!runAt) throw new Error('Pilih tanggal dan jam penjadwalan yang valid.')
      return scheduleChannelPost(selected!.chat_id, {
        bot_id: selected!.bot_id ?? '',
        text: text.trim(),
        run_at: runAt,
        parse_mode: parseMode,
      })
    },
    onSuccess: () => {
      pushToast('Postingan dijadwalkan.', 'success')
      setText('')
      setScheduleAt('')
      setScheduleMode(false)
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const channels = channelsQuery.data ?? []
  const history = historyQuery.data ?? []
  const busy = postMutation.isPending || scheduleMutation.isPending
  const runAtIso = datetimeLocalToIso(scheduleAt)

  if (channelsQuery.isLoading) return <LoadingSpinner label="Memuat daftar channel…" />
  if (channelsQuery.isError) {
    return (
      <div>
        <PageHeader title="Channels" subtitle="Posting ke channel Telegram" />
        <ErrorState message={(channelsQuery.error as Error).message} onRetry={() => void channelsQuery.refetch()} />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <PageHeader title="Channels" subtitle="Posting ke channel Telegram" />

      {channels.length === 0 ? (
        <EmptyState
          icon={<Megaphone className="h-10 w-10 text-primary" />}
          title="Belum ada channel"
          description="Channel muncul setelah bot Anda menjadi admin di sebuah channel dan ada aktivitas di sana."
        />
      ) : (
        <>
          <div className="flex gap-2 overflow-x-auto">
            {channels.map((c) => (
              <button
                key={String(c.chat_id)}
                type="button"
                onClick={() => setSelected(c)}
                className={`shrink-0 rounded-full px-4 py-2 text-sm font-semibold ${
                  String(selected?.chat_id) === String(c.chat_id)
                    ? 'bg-primary text-white'
                    : 'bg-ink/10 dark:bg-darktext/10'
                }`}
              >
                {c.title ?? `Channel ${c.chat_id}`}
              </button>
            ))}
          </div>

          {selected === null ? (
            <EmptyState
              title="Pilih channel"
              description="Pilih salah satu channel di atas untuk menulis postingan."
            />
          ) : (
            <>
              <Card>
                <h2 className="mb-1 font-bold">{selected.title ?? `Channel ${selected.chat_id}`}</h2>
                <p className="mb-3 text-sm text-ink/60 dark:text-darktext/60">
                  Bot: {selected.bot_username ? `@${selected.bot_username}` : '—'} · ID channel: {selected.chat_id}
                </p>
                <div className="space-y-3">
                  <textarea
                    value={text}
                    onChange={(e) => setText(e.target.value)}
                    placeholder="Tulis postingan channel…"
                    rows={4}
                    className={inputCls}
                  />
                  <div className="flex flex-wrap items-center gap-2">
                    <select
                      value={parseMode}
                      onChange={(e) => setParseMode(e.target.value as 'MarkdownV2' | 'HTML' | 'none')}
                      className={`${inputCls} !w-auto`}
                      aria-label="Mode format"
                    >
                      <option value="none">Tanpa format</option>
                      <option value="MarkdownV2">MarkdownV2</option>
                      <option value="HTML">HTML</option>
                    </select>
                    <button
                      type="button"
                      onClick={() => setScheduleMode((s) => !s)}
                      className={`flex items-center gap-1 rounded-full px-3 py-1.5 text-sm font-semibold ${
                        scheduleMode ? 'bg-primary text-white' : 'bg-ink/10 dark:bg-darktext/10'
                      }`}
                    >
                      <CalendarClock className="h-4 w-4" /> Jadwalkan
                    </button>
                  </div>
                  {scheduleMode && (
                    <label className="block text-sm">
                      <span className="mb-1 block font-medium">Tanggal & jam posting</span>
                      <input
                        type="datetime-local"
                        value={scheduleAt}
                        min={nowDatetimeLocal()}
                        onChange={(e) => setScheduleAt(e.target.value)}
                        className={inputCls}
                      />
                      {scheduleAt && runAtIso && (
                        <span className="mt-1 block text-xs text-ink/50 dark:text-darktext/50">
                          run_at (UTC): {runAtIso}
                        </span>
                      )}
                    </label>
                  )}
                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() => postMutation.mutate()}
                      disabled={!text.trim() || busy || scheduleMode}
                      className="flex items-center gap-1 rounded-full bg-primary px-4 py-2 text-sm font-bold text-white disabled:opacity-50"
                    >
                      <Send className="h-4 w-4" />
                      {postMutation.isPending ? 'Mengirim…' : 'Kirim Sekarang'}
                    </button>
                    {scheduleMode && (
                      <button
                        type="button"
                        onClick={() => scheduleMutation.mutate()}
                        disabled={!text.trim() || !runAtIso || busy}
                        className="flex items-center gap-1 rounded-full bg-secondary/20 px-4 py-2 text-sm font-bold text-primary disabled:opacity-50"
                      >
                        <CalendarClock className="h-4 w-4" />
                        {scheduleMutation.isPending ? 'Menjadwalkan…' : 'Jadwalkan Postingan'}
                      </button>
                    )}
                  </div>
                </div>
              </Card>

              <h2 className="font-bold">Riwayat Posting Channel</h2>
              {historyQuery.isLoading ? (
                <LoadingSpinner label="Memuat riwayat posting…" />
              ) : historyQuery.isError ? (
                <ErrorState
                  message={(historyQuery.error as Error).message}
                  onRetry={() => void historyQuery.refetch()}
                />
              ) : history.length === 0 ? (
                <EmptyState
                  title="Belum ada postingan tercatat"
                  description="Postingan ke channel ini akan muncul di sini setelah terkirim."
                />
              ) : (
                <div className="space-y-2">
                  {history.map((m) => (
                    <Card key={String(m.id)}>
                      <div className="flex flex-wrap items-center gap-1.5">
                        <Badge tone={m.direction === 'in' || m.direction === 'inbound' ? 'info' : 'success'}>
                          {m.direction === 'in' || m.direction === 'inbound' ? 'Masuk' : 'Keluar'}
                        </Badge>
                        <Badge tone={m.status === 'failed' ? 'danger' : 'default'}>{m.status ?? '—'}</Badge>
                        <span className="text-xs text-ink/50 dark:text-darktext/50">
                          {messageTypeLabel(m.message_type)}
                          {m.created_at ? ` · ${new Date(m.created_at).toLocaleString('id-ID')}` : ''}
                        </span>
                      </div>
                      <p className="mt-2 whitespace-pre-wrap text-sm">
                        {m.text ?? m.content ?? '(media tanpa teks)'}
                      </p>
                    </Card>
                  ))}
                </div>
              )}
            </>
          )}
        </>
      )}
    </div>
  )
}
