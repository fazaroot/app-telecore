import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { MessageSquare, RefreshCw, Terminal, Trash2 } from 'lucide-react'
import { fetchBots } from '../api/bots'
import {
  createCommand,
  deleteCommand,
  fetchCommands,
  fetchMessages,
  retryMessage,
  sendMessage,
  testCommand,
  updateCommand,
  type SendMessagePayload,
} from '../api/messages'
import { useUiStore } from '../stores/settings'
import { useLocalTemplates } from '../hooks/useLocalTemplates'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { ConfirmDialog, Modal } from '../components/Modal'
import { MessageComposer, type ComposerValue } from '../components/MessageComposer'
import type { Command } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

type Tab = 'compose' | 'history' | 'templates' | 'commands'

export function MessagesPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [tab, setTab] = useState<Tab>('compose')
  const [filterBot, setFilterBot] = useState('')
  const [filterStatus, setFilterStatus] = useState('')
  const [editingCommand, setEditingCommand] = useState<Command | null>(null)
  const [showCommandForm, setShowCommandForm] = useState(false)
  const [deletingCommand, setDeletingCommand] = useState<Command | null>(null)
  const [cmdBotId, setCmdBotId] = useState('')
  const [cmdName, setCmdName] = useState('')
  const [cmdDescription, setCmdDescription] = useState('')
  const [cmdResponse, setCmdResponse] = useState('')
  const [cmdTestChatId, setCmdTestChatId] = useState('')

  const { templates, addTemplate, removeTemplate } = useLocalTemplates()

  const botsQuery = useQuery({ queryKey: ['bots'], queryFn: fetchBots })
  const messagesQuery = useQuery({
    queryKey: ['messages', filterBot, filterStatus],
    queryFn: () =>
      fetchMessages({
        bot_id: filterBot || undefined,
        status: filterStatus || undefined,
      }),
    enabled: tab === 'history',
  })
  const commandsQuery = useQuery({
    queryKey: ['commands', filterBot],
    queryFn: () => fetchCommands(filterBot || undefined),
    enabled: tab === 'commands',
  })

  const sendMutation = useMutation({
    mutationFn: (v: ComposerValue) => {
      const payload: SendMessagePayload = {
        bot_id: v.botId,
        chat_id: v.chatId,
        message_type: v.messageType,
        text: v.text || undefined,
        parse_mode: v.parseMode,
        keyboard: v.keyboard,
        media: v.media,
      }
      return sendMessage(payload)
    },
    onSuccess: () => {
      pushToast('Pesan dikirim.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['messages'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const retryMutation = useMutation({
    mutationFn: (id: string | number) => retryMessage(id),
    onSuccess: () => {
      pushToast('Pesan dikirim ulang.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['messages'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const saveCommandMutation = useMutation({
    mutationFn: () => {
      const payload = {
        bot_id: cmdBotId || undefined,
        name: cmdName.trim(),
        description: cmdDescription.trim(),
        response_template: cmdResponse.trim(),
      }
      return editingCommand ? updateCommand(editingCommand.id, payload) : createCommand(payload)
    },
    onSuccess: () => {
      pushToast(editingCommand ? 'Command diperbarui.' : 'Command dibuat.', 'success')
      setShowCommandForm(false)
      setEditingCommand(null)
      void queryClient.invalidateQueries({ queryKey: ['commands'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteCommandMutation = useMutation({
    mutationFn: (id: string | number) => deleteCommand(id),
    onSuccess: () => {
      pushToast('Command dihapus.', 'success')
      setDeletingCommand(null)
      void queryClient.invalidateQueries({ queryKey: ['commands'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const testCommandMutation = useMutation({
    mutationFn: (cmd: Command) => testCommand(cmd.id, cmdTestChatId.trim()),
    onSuccess: () => pushToast('Command diuji — periksa chat tujuan.', 'success'),
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const openCommandForm = (cmd?: Command) => {
    setEditingCommand(cmd ?? null)
    setCmdBotId(cmd?.bot_id !== undefined ? String(cmd.bot_id) : '')
    setCmdName(cmd?.command ?? cmd?.name ?? '')
    setCmdDescription(cmd?.description ?? '')
    setCmdResponse(cmd?.response ?? cmd?.response_template ?? '')
    setShowCommandForm(true)
  }

  const bots = botsQuery.data ?? []
  const messages = messagesQuery.data ?? []
  const commands = commandsQuery.data ?? []

  const tabs: { key: Tab; label: string }[] = [
    { key: 'compose', label: 'Kirim' },
    { key: 'history', label: 'Riwayat' },
    { key: 'templates', label: 'Template' },
    { key: 'commands', label: 'Perintah' },
  ]

  return (
    <div className="space-y-4">
      <PageHeader title="Messages" subtitle="Kirim pesan, kelola template dan command bot" />

      <div className="flex gap-2 overflow-x-auto">
        {tabs.map((t) => (
          <button
            key={t.key}
            type="button"
            onClick={() => setTab(t.key)}
            className={`shrink-0 rounded-full px-4 py-2 text-sm font-semibold ${
              tab === t.key ? 'bg-primary text-white' : 'bg-ink/10 dark:bg-darktext/10'
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>

      {tab === 'compose' && (
        <Card>
          {botsQuery.isLoading ? (
            <LoadingSpinner label="Memuat daftar bot…" />
          ) : botsQuery.isError ? (
            <ErrorState message={(botsQuery.error as Error).message} onRetry={() => void botsQuery.refetch()} />
          ) : bots.length === 0 ? (
            <EmptyState
              title="Belum ada bot"
              description="Tambahkan bot dulu di halaman My Bots sebelum mengirim pesan."
            />
          ) : (
            <MessageComposer
              bots={bots}
              templates={templates}
              sending={sendMutation.isPending}
              onSend={(v) => sendMutation.mutate(v)}
              onSaveTemplate={(name, text, parseMode) => {
                addTemplate({ name, text, parse_mode: parseMode })
                pushToast('Template lokal disimpan di perangkat ini.', 'success')
              }}
            />
          )}
        </Card>
      )}

      {tab === 'history' && (
        <div className="space-y-3">
          <div className="flex flex-wrap gap-2">
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
            <select
              value={filterStatus}
              onChange={(e) => setFilterStatus(e.target.value)}
              className={`${inputCls} !w-auto`}
              aria-label="Filter status"
            >
              <option value="">Semua status</option>
              <option value="pending">Pending</option>
              <option value="sent">Terkirim</option>
              <option value="delivered">Diterima</option>
              <option value="failed">Gagal</option>
            </select>
          </div>
          {messagesQuery.isLoading ? (
            <LoadingSpinner label="Memuat riwayat pesan…" />
          ) : messagesQuery.isError ? (
            <ErrorState message={(messagesQuery.error as Error).message} onRetry={() => void messagesQuery.refetch()} />
          ) : messages.length === 0 ? (
            <EmptyState
              icon={<MessageSquare className="h-10 w-10 text-primary" />}
              title="Belum ada pesan"
              description="Pesan masuk dan keluar akan tercatat di sini."
            />
          ) : (
            messages.map((m) => (
              <Card key={String(m.id)}>
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <Badge tone={m.direction === 'in' || m.direction === 'inbound' ? 'info' : 'success'}>
                        {m.direction === 'in' || m.direction === 'inbound' ? 'Masuk' : 'Keluar'}
                      </Badge>
                      <Badge tone={m.status === 'failed' ? 'danger' : m.status === 'pending' ? 'warning' : 'default'}>
                        {m.status ?? '—'}
                      </Badge>
                      <span className="text-xs text-ink/50 dark:text-darktext/50">
                        {m.message_type ?? 'text'} · chat {m.chat_id ?? '—'}
                      </span>
                    </div>
                    <p className="mt-2 whitespace-pre-wrap text-sm">{m.text ?? m.content ?? '(media tanpa teks)'}</p>
                    {(m.error ?? m.error_message) && (
                      <p className="mt-1 text-xs text-danger">Error: {m.error ?? m.error_message}</p>
                    )}
                    <p className="mt-1 text-xs text-ink/50 dark:text-darktext/50">
                      {m.created_at ? new Date(m.created_at).toLocaleString('id-ID') : ''}
                    </p>
                  </div>
                  {m.status === 'failed' && (
                    <button
                      type="button"
                      onClick={() => retryMutation.mutate(m.id)}
                      disabled={retryMutation.isPending}
                      className="flex shrink-0 items-center gap-1 rounded-full bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary"
                    >
                      <RefreshCw className="h-3.5 w-3.5" /> Kirim ulang
                    </button>
                  )}
                </div>
              </Card>
            ))
          )}
        </div>
      )}

      {tab === 'templates' && (
        <div className="space-y-3">
          <p className="rounded-xl bg-secondary/10 p-3 text-sm text-ink/80 dark:text-darktext/80">
            Template di bawah ini adalah <strong>template lokal perangkat</strong> — tersimpan di localStorage
            perangkat ini saja dan tidak tersimpan di server.
          </p>
          {templates.length === 0 ? (
            <EmptyState
              title="Belum ada template"
              description="Buat template dari tab Kirim dengan tombol 'Simpan sebagai template lokal'."
            />
          ) : (
            templates.map((t) => (
              <Card key={t.id}>
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="font-bold">{t.name}</p>
                    <p className="mt-1 whitespace-pre-wrap text-sm text-ink/70 dark:text-darktext/70">{t.text}</p>
                    <p className="mt-1 text-xs text-ink/50 dark:text-darktext/50">Format: {t.parse_mode}</p>
                  </div>
                  <button
                    type="button"
                    aria-label="Hapus template"
                    onClick={() => removeTemplate(t.id)}
                    className="rounded-full p-2 text-danger hover:bg-danger/10"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </Card>
            ))
          )}
        </div>
      )}

      {tab === 'commands' && (
        <div className="space-y-3">
          <div className="flex flex-wrap gap-2">
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
            <button
              type="button"
              onClick={() => openCommandForm()}
              className="rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
            >
              + Command Baru
            </button>
          </div>
          {commandsQuery.isLoading ? (
            <LoadingSpinner label="Memuat command…" />
          ) : commandsQuery.isError ? (
            <ErrorState message={(commandsQuery.error as Error).message} onRetry={() => void commandsQuery.refetch()} />
          ) : commands.length === 0 ? (
            <EmptyState
              icon={<Terminal className="h-10 w-10 text-primary" />}
              title="Belum ada command"
              description="Command memungkinkan bot membalas pesan seperti /start atau /help secara otomatis."
            />
          ) : (
            commands.map((c) => (
              <Card key={String(c.id)}>
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="font-mono font-bold">{c.command ?? c.name ?? '—'}</p>
                    <p className="text-sm text-ink/60 dark:text-darktext/60">{c.description ?? ''}</p>
                    {(c.response ?? c.response_template) && (
                      <p className="mt-1 whitespace-pre-wrap text-sm">
                        Balasan: {c.response ?? c.response_template}
                      </p>
                    )}
                  </div>
                  <Badge tone={c.is_enabled === false ? 'default' : 'success'}>
                    {c.is_enabled === false ? 'Nonaktif' : 'Aktif'}
                  </Badge>
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  <input
                    value={cmdTestChatId}
                    onChange={(e) => setCmdTestChatId(e.target.value)}
                    placeholder="Chat ID untuk uji"
                    className={`${inputCls} !w-40`}
                  />
                  <button
                    type="button"
                    disabled={!cmdTestChatId.trim() || testCommandMutation.isPending}
                    onClick={() => testCommandMutation.mutate(c)}
                    className="rounded-full bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary disabled:opacity-50"
                  >
                    Uji
                  </button>
                  <button
                    type="button"
                    onClick={() => openCommandForm(c)}
                    className="rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    onClick={() => setDeletingCommand(c)}
                    className="rounded-full bg-danger/10 px-3 py-1.5 text-xs font-semibold text-danger"
                  >
                    Hapus
                  </button>
                </div>
              </Card>
            ))
          )}
        </div>
      )}

      <Modal open={showCommandForm} onClose={() => setShowCommandForm(false)} title={editingCommand ? 'Edit Command' : 'Command Baru'}>
        <div className="space-y-3">
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Bot</span>
            <select value={cmdBotId} onChange={(e) => setCmdBotId(e.target.value)} className={inputCls}>
              <option value="">Pilih bot…</option>
              {bots.map((b) => (
                <option key={String(b.id)} value={String(b.id)}>
                  {b.name ?? b.username ?? `Bot ${b.id}`}
                </option>
              ))}
            </select>
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Command</span>
            <input value={cmdName} onChange={(e) => setCmdName(e.target.value)} placeholder="/start" className={inputCls} />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Deskripsi</span>
            <input value={cmdDescription} onChange={(e) => setCmdDescription(e.target.value)} placeholder="Deskripsi singkat" className={inputCls} />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Balasan otomatis</span>
            <textarea value={cmdResponse} onChange={(e) => setCmdResponse(e.target.value)} rows={3} placeholder="Teks balasan bot" className={inputCls} />
          </label>
          <button
            type="button"
            disabled={!cmdBotId || !cmdName.trim() || saveCommandMutation.isPending}
            onClick={() => saveCommandMutation.mutate()}
            className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
          >
            {saveCommandMutation.isPending ? 'Menyimpan…' : 'Simpan Command'}
          </button>
        </div>
      </Modal>

      <ConfirmDialog
        open={deletingCommand !== null}
        onClose={() => setDeletingCommand(null)}
        onConfirm={() => deletingCommand && deleteCommandMutation.mutate(deletingCommand.id)}
        title="Hapus Command"
        message={`Hapus command "${deletingCommand?.command ?? deletingCommand?.name ?? ''}"?`}
        busy={deleteCommandMutation.isPending}
      />
    </div>
  )
}
