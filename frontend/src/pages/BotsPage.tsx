import { useRef, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Bot as BotIcon, Download, Plug, Plus, RefreshCw, Settings2, Trash2, Upload, Wifi, WifiOff } from 'lucide-react'
import {
  connectBot,
  createBot,
  deleteBot,
  disconnectBot,
  exportBots,
  fetchBotConfig,
  fetchBots,
  importBots,
  testBot,
  updateBot,
  updateBotConfig,
} from '../api/bots'
import { downloadTextFile } from '../api/client'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { ConfirmDialog, Modal } from '../components/Modal'
import type { Bot, BotConfig } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

function statusBadge(status?: string) {
  switch (status) {
    case 'connected':
      return <Badge tone="success">Terhubung</Badge>
    case 'disconnected':
      return <Badge tone="warning">Terputus</Badge>
    case 'error':
      return <Badge tone="danger">Error</Badge>
    default:
      return <Badge>Belum diuji</Badge>
  }
}

export function BotsPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [showAdd, setShowAdd] = useState(false)
  const [editing, setEditing] = useState<Bot | null>(null)
  const [configBot, setConfigBot] = useState<Bot | null>(null)
  const [deleting, setDeleting] = useState<Bot | null>(null)
  const [newName, setNewName] = useState('')
  const [newToken, setNewToken] = useState('')
  const [editName, setEditName] = useState('')
  const fileRef = useRef<HTMLInputElement>(null)

  const botsQuery = useQuery({ queryKey: ['bots'], queryFn: fetchBots })
  const configQuery = useQuery({
    queryKey: ['bot-config', configBot?.id],
    queryFn: () => fetchBotConfig(configBot!.id),
    enabled: configBot !== null,
  })

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['bots'] })

  const createMutation = useMutation({
    mutationFn: () => createBot({ name: newName.trim(), token: newToken.trim() }),
    onSuccess: () => {
      pushToast('Bot ditambahkan. Backend memvalidasi token via getMe.', 'success')
      setShowAdd(false)
      setNewName('')
      setNewToken('')
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const updateMutation = useMutation({
    mutationFn: () => updateBot(editing!.id, { name: editName.trim() }),
    onSuccess: () => {
      pushToast('Bot diperbarui.', 'success')
      setEditing(null)
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string | number) => deleteBot(id),
    onSuccess: () => {
      pushToast('Bot dihapus.', 'success')
      setDeleting(null)
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const testMutation = useMutation({
    mutationFn: (id: string | number) => testBot(id),
    onSuccess: (data) => {
      pushToast(`Uji koneksi selesai: ${JSON.stringify(data).slice(0, 120)}`, 'success')
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const toggleActiveMutation = useMutation({
    mutationFn: (bot: Bot) => updateBot(bot.id, { is_active: !bot.is_active }),
    onSuccess: () => {
      pushToast('Status bot diperbarui.', 'success')
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const connectMutation = useMutation({
    mutationFn: (bot: Bot) =>
      bot.connection_status === 'connected' ? disconnectBot(bot.id) : connectBot(bot.id),
    onSuccess: () => {
      pushToast('Status koneksi diperbarui.', 'success')
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const configMutation = useMutation({
    mutationFn: (cfg: BotConfig) => updateBotConfig(configBot!.id, cfg),
    onSuccess: () => {
      pushToast('Pengaturan bot disimpan.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['bot-config', configBot?.id] })
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const exportMutation = useMutation({
    mutationFn: exportBots,
    onSuccess: (data) => {
      downloadTextFile('telecore-bots-export.json', JSON.stringify(data, null, 2))
      pushToast('Konfigurasi bot diekspor. Token tersimpan dalam bentuk terenkripsi.', 'success')
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const importMutation = useMutation({
    mutationFn: (payload: unknown) => importBots(payload),
    onSuccess: () => {
      pushToast('Konfigurasi bot diimpor.', 'success')
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const handleImportFile = (file: File) => {
    const reader = new FileReader()
    reader.onload = () => {
      try {
        importMutation.mutate(JSON.parse(String(reader.result ?? '')))
      } catch {
        pushToast('File JSON tidak valid.', 'error')
      }
    }
    reader.readAsText(file)
  }

  const config = configQuery.data
  // Polling & webhook saling eksklusif: mengaktifkan satu menonaktifkan yang lain.
  const setPolling = (enabled: boolean) =>
    configMutation.mutate({
      ...(config ?? {}),
      polling_enabled: enabled,
      webhook_enabled: enabled ? false : config?.webhook_enabled,
    })
  const setWebhookEnabled = (enabled: boolean) =>
    configMutation.mutate({
      ...(config ?? {}),
      webhook_enabled: enabled,
      polling_enabled: enabled ? false : config?.polling_enabled,
    })

  if (botsQuery.isLoading) return <LoadingSpinner label="Memuat daftar bot…" />
  if (botsQuery.isError) {
    return (
      <div>
        <PageHeader title="My Bots" subtitle="Kelola bot Telegram Anda" />
        <ErrorState message={(botsQuery.error as Error).message} onRetry={() => void botsQuery.refetch()} />
      </div>
    )
  }

  const bots = botsQuery.data ?? []

  return (
    <div className="space-y-4">
      <PageHeader
        title="My Bots"
        subtitle="Kelola bot Telegram Anda"
        action={
          <button
            type="button"
            onClick={() => setShowAdd(true)}
            className="flex items-center gap-1 rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
          >
            <Plus className="h-4 w-4" /> Tambah Bot
          </button>
        }
      />

      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => exportMutation.mutate()}
          disabled={exportMutation.isPending}
          className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-sm font-semibold dark:bg-darktext/10"
        >
          <Download className="h-4 w-4" /> Export JSON
        </button>
        <button
          type="button"
          onClick={() => fileRef.current?.click()}
          disabled={importMutation.isPending}
          className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-sm font-semibold dark:bg-darktext/10"
        >
          <Upload className="h-4 w-4" /> Import JSON
        </button>
        <input
          ref={fileRef}
          type="file"
          accept="application/json,.json"
          className="hidden"
          onChange={(e) => {
            const f = e.target.files?.[0]
            if (f) handleImportFile(f)
            e.target.value = ''
          }}
        />
        <p className="w-full text-xs text-ink/50 dark:text-darktext/50">
          Peringatan: file export berisi token dalam bentuk terenkripsi. Simpan file ini dengan aman dan jangan
          dibagikan.
        </p>
      </div>

      {bots.length === 0 ? (
        <EmptyState
          icon={<BotIcon className="h-10 w-10 text-primary" />}
          title="Belum ada bot"
          description="Tambahkan bot pertama Anda dengan token dari @BotFather. Backend akan memvalidasi token via getMe."
          action={
            <button
              type="button"
              onClick={() => setShowAdd(true)}
              className="mt-2 rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
            >
              Tambah Bot
            </button>
          }
        />
      ) : (
        <div className="space-y-3">
          {bots.map((bot) => (
            <Card key={String(bot.id)}>
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="truncate font-bold">{bot.name ?? 'Bot tanpa nama'}</p>
                  <p className="truncate text-sm text-ink/60 dark:text-darktext/60">
                    {bot.username ? `@${bot.username}` : `ID Telegram: ${bot.telegram_id ?? '—'}`}
                  </p>
                  <p className="mt-1 font-mono text-xs text-ink/50 dark:text-darktext/50">
                    Token: {bot.token_masked ?? '****'}
                  </p>
                  <div className="mt-2 flex flex-wrap gap-1.5">
                    {statusBadge(bot.connection_status)}
                    <Badge tone={bot.is_active ? 'success' : 'default'}>{bot.is_active ? 'Aktif' : 'Nonaktif'}</Badge>
                    {bot.webhook_enabled && <Badge tone="info">Webhook</Badge>}
                  </div>
                </div>
                <button
                  type="button"
                  aria-label="Pengaturan bot"
                  onClick={() => setConfigBot(bot)}
                  className="rounded-full p-2 hover:bg-ink/10 dark:hover:bg-darktext/10"
                >
                  <Settings2 className="h-5 w-5" />
                </button>
              </div>
              <div className="mt-3 flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => connectMutation.mutate(bot)}
                  disabled={connectMutation.isPending}
                  className="flex items-center gap-1 rounded-full bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary"
                >
                  {bot.connection_status === 'connected' ? <WifiOff className="h-3.5 w-3.5" /> : <Wifi className="h-3.5 w-3.5" />}
                  {bot.connection_status === 'connected' ? 'Putuskan' : 'Hubungkan'}
                </button>
                <button
                  type="button"
                  onClick={() => testMutation.mutate(bot.id)}
                  disabled={testMutation.isPending}
                  className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
                >
                  <Plug className="h-3.5 w-3.5" /> Uji Koneksi
                </button>
                <button
                  type="button"
                  onClick={() => toggleActiveMutation.mutate(bot)}
                  disabled={toggleActiveMutation.isPending}
                  className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
                >
                  <RefreshCw className="h-3.5 w-3.5" /> {bot.is_active ? 'Nonaktifkan' : 'Aktifkan'}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setEditing(bot)
                    setEditName(bot.name ?? '')
                  }}
                  className="rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
                >
                  Edit Nama
                </button>
                <button
                  type="button"
                  onClick={() => setDeleting(bot)}
                  className="flex items-center gap-1 rounded-full bg-danger/10 px-3 py-1.5 text-xs font-semibold text-danger"
                >
                  <Trash2 className="h-3.5 w-3.5" /> Hapus
                </button>
              </div>
            </Card>
          ))}
        </div>
      )}

      {/* Tambah bot */}
      <Modal open={showAdd} onClose={() => setShowAdd(false)} title="Tambah Bot">
        <div className="space-y-3">
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Nama bot</span>
            <input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="mis. Bot Toko" className={inputCls} />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Token Bot (dari @BotFather)</span>
            <input
              value={newToken}
              onChange={(e) => setNewToken(e.target.value)}
              placeholder="123456:ABC-DEF…"
              type="password"
              autoComplete="off"
              className={inputCls}
            />
          </label>
          <p className="text-xs text-ink/50 dark:text-darktext/50">
            Token dikirim ke backend dan divalidasi via getMe Telegram. Token disimpan terenkripsi di server.
          </p>
          <button
            type="button"
            disabled={!newName.trim() || !newToken.trim() || createMutation.isPending}
            onClick={() => createMutation.mutate()}
            className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
          >
            {createMutation.isPending ? 'Memvalidasi…' : 'Tambah & Validasi Bot'}
          </button>
        </div>
      </Modal>

      {/* Edit nama */}
      <Modal open={editing !== null} onClose={() => setEditing(null)} title="Edit Bot">
        <div className="space-y-3">
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Nama bot</span>
            <input value={editName} onChange={(e) => setEditName(e.target.value)} className={inputCls} />
          </label>
          <button
            type="button"
            disabled={!editName.trim() || updateMutation.isPending}
            onClick={() => updateMutation.mutate()}
            className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
          >
            {updateMutation.isPending ? 'Menyimpan…' : 'Simpan'}
          </button>
        </div>
      </Modal>

      {/* Pengaturan webhook & polling */}
      <Modal open={configBot !== null} onClose={() => setConfigBot(null)} title={`Pengaturan — ${configBot?.name ?? ''}`}>
        {configQuery.isLoading ? (
          <LoadingSpinner label="Memuat pengaturan…" />
        ) : configQuery.isError ? (
          <ErrorState message={(configQuery.error as Error).message} onRetry={() => void configQuery.refetch()} />
        ) : (
          <div className="space-y-4">
            <p className="rounded-xl bg-primary/10 p-3 text-sm text-ink/80 dark:text-darktext/80">
              Polling dan webhook saling eksklusif: mengaktifkan salah satu akan menonaktifkan yang lain.
              Polling cocok untuk server lokal; webhook membutuhkan URL publik yang bisa dijangkau Telegram.
            </p>
            <label className="flex items-center justify-between gap-3">
              <span className="text-sm font-medium">Polling aktif</span>
              <input
                type="checkbox"
                checked={Boolean(config?.polling_enabled ?? configBot?.polling_enabled)}
                onChange={(e) => setPolling(e.target.checked)}
                disabled={configMutation.isPending}
                className="h-5 w-5 accent-primary"
              />
            </label>
            <label className="flex items-center justify-between gap-3">
              <span className="text-sm font-medium">Webhook aktif</span>
              <input
                type="checkbox"
                checked={Boolean(config?.webhook_enabled ?? configBot?.webhook_enabled)}
                onChange={(e) => setWebhookEnabled(e.target.checked)}
                disabled={configMutation.isPending}
                className="h-5 w-5 accent-primary"
              />
            </label>
            <label className="block text-sm">
              <span className="mb-1 block font-medium">URL Webhook</span>
              <input
                defaultValue={config?.webhook_url ?? configBot?.webhook_url ?? ''}
                placeholder="https://domain-anda.com/webhook"
                className={inputCls}
                onBlur={(e) => {
                  const url = e.target.value.trim()
                  if (url !== (config?.webhook_url ?? '')) {
                    configMutation.mutate({ ...(config ?? {}), webhook_url: url || null })
                  }
                }}
              />
            </label>
            {configMutation.isPending && <p className="text-sm text-ink/60 dark:text-darktext/60">Menyimpan…</p>}
          </div>
        )}
      </Modal>

      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && deleteMutation.mutate(deleting.id)}
        title="Hapus Bot"
        message={`Hapus bot "${deleting?.name ?? ''}"? Riwayat pesan dan automation terkait bot ini ikut terhapus di server.`}
        busy={deleteMutation.isPending}
      />
    </div>
  )
}
