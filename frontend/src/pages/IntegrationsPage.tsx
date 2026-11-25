import { useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CheckCircle2, Copy, Link2, Plug, Send, Trash2, Webhook } from 'lucide-react'
import {
  OUTGOING_WEBHOOK_EVENTS,
  buildClaudeDesktopConfig,
  claudeDesktopConfigJson,
  createOutgoingWebhook,
  deleteOutgoingWebhook,
  fetchOutgoingWebhooks,
  fetchWebhookDeliveries,
  retryWebhookDelivery,
  testMcpConnection,
  testOutgoingWebhook,
  updateOutgoingWebhook,
} from '../api/integrations'
import { getBaseUrl } from '../api/client'
import { useSettingsStore, useUiStore } from '../stores/settings'
import { copyText } from '../utils/copy'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { ConfirmDialog, Modal } from '../components/Modal'
import type { OutgoingWebhook } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

export function IntegrationsPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const baseUrl = useSettingsStore((s) => s.baseUrl)
  const apiKey = useSettingsStore((s) => s.apiKey)

  const mcpUrl = useMemo(() => `${getBaseUrl()}/mcp`, [baseUrl])
  const [mcpTested, setMcpTested] = useState<'idle' | 'ok' | 'fail'>('idle')
  const [mcpTestBusy, setMcpTestBusy] = useState(false)

  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<OutgoingWebhook | null>(null)
  const [deleting, setDeleting] = useState<OutgoingWebhook | null>(null)
  const [selectedId, setSelectedId] = useState<string>('')
  const [createdSecret, setCreatedSecret] = useState<string | null>(null)
  const [formName, setFormName] = useState('')
  const [formUrl, setFormUrl] = useState('')
  const [formEvents, setFormEvents] = useState<string[]>(['message.inbound'])
  const [formEnabled, setFormEnabled] = useState(true)

  const webhooksQuery = useQuery({ queryKey: ['outgoing-webhooks'], queryFn: fetchOutgoingWebhooks })
  const deliveriesQuery = useQuery({
    queryKey: ['webhook-deliveries', selectedId],
    queryFn: () => fetchWebhookDeliveries(selectedId),
    enabled: Boolean(selectedId),
  })

  const saveMutation = useMutation({
    mutationFn: () => {
      const payload = { name: formName.trim(), url: formUrl.trim(), events: formEvents, is_enabled: formEnabled }
      return editing ? updateOutgoingWebhook(editing.id, payload) : createOutgoingWebhook(payload)
    },
    onSuccess: (wh) => {
      pushToast(editing ? 'Webhook diperbarui.' : 'Webhook dibuat.', 'success')
      // Secret hanya ditampilkan sekali saat webhook dibuat.
      if (!editing && wh.secret) setCreatedSecret(wh.secret)
      setShowForm(false)
      setEditing(null)
      void queryClient.invalidateQueries({ queryKey: ['outgoing-webhooks'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string | number) => deleteOutgoingWebhook(id),
    onSuccess: () => {
      pushToast('Webhook dihapus.', 'success')
      setDeleting(null)
      if (selectedId && deleting && String(deleting.id) === selectedId) setSelectedId('')
      void queryClient.invalidateQueries({ queryKey: ['outgoing-webhooks'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const testMutation = useMutation({
    mutationFn: (id: string | number) => testOutgoingWebhook(id),
    onSuccess: () => {
      pushToast('Webhook uji terkirim.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['webhook-deliveries', selectedId] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const retryMutation = useMutation({
    mutationFn: (deliveryId: string | number) => retryWebhookDelivery(selectedId, deliveryId),
    onSuccess: () => {
      pushToast('Delivery dikirim ulang.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['webhook-deliveries', selectedId] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const openForm = (wh?: OutgoingWebhook) => {
    setEditing(wh ?? null)
    setFormName(wh?.name ?? '')
    setFormUrl(wh?.url ?? '')
    setFormEvents(wh?.events ?? ['message.inbound'])
    setFormEnabled(wh?.is_enabled ?? wh?.enabled ?? true)
    setShowForm(true)
  }

  const copyConfig = async () => {
    if (!apiKey) {
      pushToast('Isi API key dulu di Settings agar konfigurasi berisi kredensial.', 'error')
      return
    }
    const ok = await copyText(claudeDesktopConfigJson(mcpUrl, apiKey))
    pushToast(
      ok ? 'Konfigurasi Claude Desktop disalin.' : 'Gagal menyalin — salin manual dari kotak di bawah.',
      ok ? 'success' : 'error',
    )
  }

  const handleMcpTest = async () => {
    setMcpTestBusy(true)
    setMcpTested('idle')
    try {
      await testMcpConnection()
      setMcpTested('ok')
      pushToast('MCP server merespons handshake initialize.', 'success')
    } catch (e) {
      setMcpTested('fail')
      pushToast(`Tes MCP gagal: ${(e as Error).message}`, 'error')
    } finally {
      setMcpTestBusy(false)
    }
  }

  const webhooks = webhooksQuery.data ?? []
  const deliveries = deliveriesQuery.data ?? []
  const selected = webhooks.find((w) => String(w.id) === selectedId)

  return (
    <div className="space-y-4">
      <PageHeader title="Integrations" subtitle="MCP server dan outgoing webhooks" />

      <Card>
        <h2 className="mb-2 flex items-center gap-2 font-bold">
          <Plug className="h-5 w-5 text-primary" /> MCP Server
        </h2>
        <p className="mb-3 text-sm text-ink/70 dark:text-darktext/70">
          TeleCore menyediakan server MCP (Model Context Protocol) di endpoint berikut. Klien MCP seperti
          Claude Desktop dapat memanggil tools TeleCore lewat endpoint ini dengan header{' '}
          <code className="rounded bg-ink/10 px-1 dark:bg-darktext/10">X-API-Key</code>.
        </p>
        <label className="mb-2 block text-sm">
          <span className="mb-1 block font-medium">Endpoint MCP</span>
          <div className="flex gap-2">
            <input value={mcpUrl} readOnly className={`${inputCls} font-mono`} />
            <button
              type="button"
              onClick={async () => {
                const ok = await copyText(mcpUrl)
                pushToast(ok ? 'Endpoint disalin.' : 'Gagal menyalin.', ok ? 'success' : 'error')
              }}
              className="flex shrink-0 items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
            >
              <Copy className="h-3.5 w-3.5" /> Salin
            </button>
          </div>
        </label>
        <p className="mb-2 text-sm font-medium">Konfigurasi Claude Desktop</p>
        <pre className="mb-2 max-h-48 overflow-auto rounded-xl bg-ink/5 p-3 font-mono text-xs dark:bg-darktext/5">
          {JSON.stringify(buildClaudeDesktopConfig(mcpUrl, apiKey ? '*** (API key tersimpan)' : ''), null, 2)}
        </pre>
        <p className="mb-3 text-xs text-ink/50 dark:text-darktext/50">
          Tempel konfigurasi ini ke file konfigurasi Claude Desktop (bagian{' '}
          <code>mcpServers</code>). Tombol salin di bawah menyertakan API key yang tersimpan di
          Settings — jangan bagikan hasil salinan itu ke orang lain.
        </p>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={copyConfig}
            disabled={!apiKey}
            title={apiKey ? undefined : 'Isi API key di Settings dulu'}
            className="flex items-center gap-1 rounded-full bg-primary px-4 py-2 text-sm font-bold text-white disabled:opacity-50"
          >
            <Copy className="h-4 w-4" /> Salin konfigurasi JSON
          </button>
          <button
            type="button"
            onClick={handleMcpTest}
            disabled={mcpTestBusy}
            className="flex items-center gap-1 rounded-full bg-ink/10 px-4 py-2 text-sm font-bold dark:bg-darktext/10 disabled:opacity-50"
          >
            <Link2 className="h-4 w-4" />
            {mcpTestBusy ? 'Menguji…' : 'Tes koneksi MCP'}
          </button>
          {mcpTested === 'ok' && (
            <span className="flex items-center gap-1 text-sm font-medium text-success">
              <CheckCircle2 className="h-4 w-4" /> MCP merespons
            </span>
          )}
        </div>
      </Card>

      {createdSecret && (
        <Card className="border-amber-500/40">
          <h2 className="mb-2 font-bold text-amber-700 dark:text-amber-400">Secret webhook — tampil sekali saja</h2>
          <p className="mb-2 text-sm">
            Simpan secret ini sekarang. Secret tidak akan ditampilkan lagi setelah kartu ini ditutup.
          </p>
          <div className="flex gap-2">
            <input value={createdSecret} readOnly className={`${inputCls} font-mono`} />
            <button
              type="button"
              onClick={async () => {
                const ok = await copyText(createdSecret)
                pushToast(ok ? 'Secret disalin.' : 'Gagal menyalin.', ok ? 'success' : 'error')
              }}
              className="flex shrink-0 items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
            >
              <Copy className="h-3.5 w-3.5" /> Salin
            </button>
          </div>
          <button
            type="button"
            onClick={() => setCreatedSecret(null)}
            className="mt-3 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
          >
            Saya sudah menyimpan secret — tutup
          </button>
        </Card>
      )}

      <Card>
        <div className="mb-3 flex items-center justify-between gap-3">
          <h2 className="flex items-center gap-2 font-bold">
            <Webhook className="h-5 w-5 text-primary" /> Outgoing Webhooks
          </h2>
          <button
            type="button"
            onClick={() => openForm()}
            className="rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
          >
            + Webhook Baru
          </button>
        </div>
        {webhooksQuery.isLoading ? (
          <LoadingSpinner label="Memuat webhooks…" />
        ) : webhooksQuery.isError ? (
          <ErrorState message={(webhooksQuery.error as Error).message} onRetry={() => void webhooksQuery.refetch()} />
        ) : webhooks.length === 0 ? (
          <EmptyState
            title="Belum ada webhook"
            description="Webhook mengirim event TeleCore (pesan masuk, automation, dsb.) ke URL server lain."
          />
        ) : (
          <ul className="space-y-2">
            {webhooks.map((w) => (
              <li key={String(w.id)} className="rounded-xl border border-ink/10 p-3 dark:border-darktext/10">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="font-semibold">{w.name ?? 'Webhook'}</p>
                    <p className="truncate font-mono text-xs text-ink/60 dark:text-darktext/60">{w.url ?? '—'}</p>
                    <div className="mt-1 flex flex-wrap gap-1">
                      {(w.events ?? []).map((e) => (
                        <Badge key={e} tone="info">{e}</Badge>
                      ))}
                      <Badge tone={(w.is_enabled ?? w.enabled ?? true) === false ? 'default' : 'success'}>
                        {(w.is_enabled ?? w.enabled ?? true) === false ? 'Nonaktif' : 'Aktif'}
                      </Badge>
                    </div>
                  </div>
                  <button
                    type="button"
                    aria-label="Hapus webhook"
                    onClick={() => setDeleting(w)}
                    className="rounded-full p-2 text-danger hover:bg-danger/10"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => openForm(w)}
                    className="rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setSelectedId(String(w.id))
                      setCreatedSecret(null)
                    }}
                    className="rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
                  >
                    Lihat deliveries
                  </button>
                  <button
                    type="button"
                    disabled={testMutation.isPending}
                    onClick={() => {
                      setSelectedId(String(w.id))
                      testMutation.mutate(w.id)
                    }}
                    className="flex items-center gap-1 rounded-full bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary disabled:opacity-50"
                  >
                    <Send className="h-3.5 w-3.5" /> Kirim uji
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Card>

      {selected && (
        <Card>
          <h2 className="mb-2 font-bold">Deliveries — {selected.name ?? selected.url}</h2>
          {deliveriesQuery.isLoading ? (
            <LoadingSpinner label="Memuat deliveries…" />
          ) : deliveriesQuery.isError ? (
            <ErrorState message={(deliveriesQuery.error as Error).message} onRetry={() => void deliveriesQuery.refetch()} />
          ) : deliveries.length === 0 ? (
            <EmptyState title="Belum ada delivery" description="Delivery tercatat setiap kali webhook mengirim event." />
          ) : (
            <ul className="space-y-2">
              {deliveries.map((d) => (
                <li key={String(d.id)} className="flex items-start justify-between gap-3 rounded-xl border border-ink/10 p-3 text-sm dark:border-darktext/10">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <Badge tone={d.status === 'failed' ? 'danger' : d.status === 'delivered' || d.status === 'success' ? 'success' : 'default'}>
                        {d.status ?? '—'}
                      </Badge>
                      <span className="text-ink/60 dark:text-darktext/60">{d.event ?? ''}</span>
                      {(d.http_status ?? d.response_status ?? d.status_code) !== undefined && (d.http_status ?? d.response_status ?? d.status_code) !== null && (
                        <span className="text-ink/60 dark:text-darktext/60">HTTP {d.http_status ?? d.response_status ?? d.status_code}</span>
                      )}
                      {d.attempts !== undefined && d.attempts > 1 && (
                        <span className="text-ink/60 dark:text-darktext/60">{d.attempts}x percobaan</span>
                      )}
                    </div>
                    {(d.last_error ?? d.error) && <p className="mt-1 text-xs text-danger">{d.last_error ?? d.error}</p>}
                    <p className="mt-1 text-xs text-ink/50 dark:text-darktext/50">
                      {d.created_at ? new Date(d.created_at).toLocaleString('id-ID') : ''}
                    </p>
                  </div>
                  {d.status === 'failed' && (
                    <button
                      type="button"
                      disabled={retryMutation.isPending}
                      onClick={() => retryMutation.mutate(d.id)}
                      className="shrink-0 rounded-full bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary disabled:opacity-50"
                    >
                      Retry
                    </button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </Card>
      )}

      <Modal open={showForm} onClose={() => setShowForm(false)} title={editing ? 'Edit Webhook' : 'Webhook Baru'}>
        <div className="space-y-3">
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Nama</span>
            <input value={formName} onChange={(e) => setFormName(e.target.value)} placeholder="mis. Notifikasi ke n8n" className={inputCls} />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">URL tujuan</span>
            <input value={formUrl} onChange={(e) => setFormUrl(e.target.value)} placeholder="https://example.com/hook" inputMode="url" className={inputCls} />
          </label>
          <fieldset>
            <legend className="mb-1 text-sm font-medium">Event</legend>
            <div className="space-y-1.5">
              {OUTGOING_WEBHOOK_EVENTS.map((e) => (
                <label key={e} className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={formEvents.includes(e)}
                    onChange={(ev) =>
                      setFormEvents((cur) =>
                        ev.target.checked ? [...cur, e] : cur.filter((x) => x !== e),
                      )
                    }
                  />
                  <span className="font-mono text-xs">{e}</span>
                </label>
              ))}
            </div>
          </fieldset>
          <label className="flex items-center gap-2 text-sm font-medium">
            <input type="checkbox" checked={formEnabled} onChange={(e) => setFormEnabled(e.target.checked)} />
            Webhook aktif
          </label>
          <button
            type="button"
            disabled={!formName.trim() || !formUrl.trim() || formEvents.length === 0 || saveMutation.isPending}
            onClick={() => saveMutation.mutate()}
            className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
          >
            {saveMutation.isPending ? 'Menyimpan…' : 'Simpan Webhook'}
          </button>
        </div>
      </Modal>

      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && deleteMutation.mutate(deleting.id)}
        title="Hapus Webhook"
        message={`Hapus webhook "${deleting?.name ?? deleting?.url ?? ''}"? Delivery yang sudah tercatat ikut terhapus.`}
        busy={deleteMutation.isPending}
      />
    </div>
  )
}
