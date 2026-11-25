import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Brain, Pencil, Plus, Trash2 } from 'lucide-react'
import {
  AI_PROVIDER_PRESETS,
  AI_PROVIDER_TYPE_LABELS,
  applyAiPreset,
  createAiProvider,
  deleteAiProvider,
  fetchAiProviders,
  maskApiKey,
  testAiProvider,
  updateAiProvider,
  type AiProviderPayload,
} from '../api/ai'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner } from './ui'
import { ConfirmDialog, Modal } from './Modal'
import type { AiProvider, AiProviderType } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

export function AiProvidersSection() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<AiProvider | null>(null)
  const [deleting, setDeleting] = useState<AiProvider | null>(null)
  const [name, setName] = useState('')
  const [presetKey, setPresetKey] = useState('')
  const [providerType, setProviderType] = useState<AiProviderType | string>('openai_compatible')
  const [baseUrl, setBaseUrl] = useState('')
  const [model, setModel] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [enabled, setEnabled] = useState(true)
  const [testResult, setTestResult] = useState<Record<string, string>>({})

  const providersQuery = useQuery({ queryKey: ['ai-providers'], queryFn: fetchAiProviders })

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['ai-providers'] })

  const saveMutation = useMutation({
    mutationFn: () => {
      const body: AiProviderPayload = {
        name: name.trim(),
        provider_type: providerType,
        base_url: baseUrl.trim(),
        model: model.trim(),
        enabled,
        ...(apiKey.trim() ? { api_key: apiKey.trim() } : {}),
      }
      return editing ? updateAiProvider(editing.id, body) : createAiProvider(body)
    },
    onSuccess: () => {
      pushToast(editing ? 'Provider diperbarui.' : 'Provider ditambahkan.', 'success')
      setShowForm(false)
      setEditing(null)
      setApiKey('')
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string | number) => deleteAiProvider(id),
    onSuccess: () => {
      pushToast('Provider dihapus.', 'success')
      setDeleting(null)
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const testMutation = useMutation({
    mutationFn: (id: string | number) => testAiProvider(id),
    onSuccess: (res, id) => {
      const msg = res.message ?? (res.ok ? 'Koneksi berhasil.' : 'Tes selesai, periksa pesan server.')
      setTestResult((prev) => ({ ...prev, [String(id)]: msg }))
      pushToast(msg, res.ok === false ? 'error' : 'success')
    },
    onError: (e: Error, id) => {
      setTestResult((prev) => ({ ...prev, [String(id)]: e.message }))
      pushToast(e.message, 'error')
    },
  })

  const openForm = (p?: AiProvider) => {
    setEditing(p ?? null)
    setName(p?.name ?? '')
    setProviderType((p?.provider_type ?? 'openai_compatible') as AiProviderType)
    setBaseUrl(p?.base_url ?? '')
    setModel(p?.model ?? '')
    setApiKey('')
    setEnabled(p?.enabled ?? true)
    setPresetKey('')
    setShowForm(true)
  }

  const applyPreset = (key: string) => {
    const preset = applyAiPreset(key)
    if (!preset) return
    setPresetKey(key)
    setProviderType(preset.provider_type)
    setBaseUrl(preset.base_url)
    setModel(preset.model)
  }

  const providers = providersQuery.data ?? []
  const formValid = name.trim() && baseUrl.trim() && model.trim() && (editing !== null || apiKey.trim())

  return (
    <Card>
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <h2 className="flex items-center gap-2 font-bold">
            <Brain className="h-5 w-5" /> AI Providers
          </h2>
          <p className="mt-1 text-sm text-ink/60 dark:text-darktext/60">
            Kunci API untuk Smart Reply bot. Kunci disimpan terenkripsi di server dan tidak pernah
            ditampilkan penuh.
          </p>
        </div>
        <button
          type="button"
          onClick={() => openForm()}
          className="flex shrink-0 items-center gap-1 rounded-full bg-primary px-3 py-1.5 text-sm font-semibold text-white"
        >
          <Plus className="h-4 w-4" /> Tambah
        </button>
      </div>

      {providersQuery.isLoading ? (
        <LoadingSpinner label="Memuat AI provider…" />
      ) : providersQuery.isError ? (
        <ErrorState message={(providersQuery.error as Error).message} onRetry={() => void providersQuery.refetch()} />
      ) : providers.length === 0 ? (
        <EmptyState
          title="Belum ada AI provider"
          description="Tambahkan provider AI (OpenAI, Anthropic, Gemini, DeepSeek, Groq, atau OpenRouter) agar bot bisa membalas otomatis."
        />
      ) : (
        <ul className="space-y-2">
          {providers.map((p) => (
            <li
              key={String(p.id)}
              className="rounded-xl border border-ink/10 p-3 dark:border-darktext/10"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className="truncate font-semibold">{p.name ?? `Provider ${p.id}`}</p>
                  <p className="text-sm text-ink/60 dark:text-darktext/60">
                    {AI_PROVIDER_TYPE_LABELS[p.provider_type ?? ''] ?? p.provider_type ?? '—'} ·{' '}
                    {p.model ?? '—'}
                  </p>
                  <p className="truncate font-mono text-xs text-ink/50 dark:text-darktext/50">
                    {p.base_url ?? ''} · API key {maskApiKey(p.api_key_last4)}
                  </p>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <Badge tone={p.enabled ? 'success' : 'default'}>{p.enabled ? 'Aktif' : 'Nonaktif'}</Badge>
                  <button
                    type="button"
                    aria-label="Edit provider"
                    onClick={() => openForm(p)}
                    className="rounded-full p-1.5 hover:bg-ink/10 dark:hover:bg-darktext/10"
                  >
                    <Pencil className="h-4 w-4" />
                  </button>
                  <button
                    type="button"
                    aria-label="Hapus provider"
                    onClick={() => setDeleting(p)}
                    className="rounded-full p-1.5 text-danger hover:bg-danger/10"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </div>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <button
                  type="button"
                  onClick={() => testMutation.mutate(p.id)}
                  disabled={testMutation.isPending}
                  className="rounded-full bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary disabled:opacity-50"
                >
                  {testMutation.isPending ? 'Menguji…' : 'Tes koneksi'}
                </button>
                {testResult[String(p.id)] && (
                  <span className="text-xs text-ink/60 dark:text-darktext/60">{testResult[String(p.id)]}</span>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}

      <p className="mt-3 rounded-xl bg-primary/10 p-3 text-sm text-ink/80 dark:text-darktext/80">
        Biaya pemakaian API AI ditanggung ke provider masing-masing; kunci disimpan terenkripsi di
        server.
      </p>

      <Modal open={showForm} onClose={() => setShowForm(false)} title={editing ? 'Edit AI Provider' : 'Tambah AI Provider'}>
        <div className="space-y-3">
          <div>
            <p className="mb-2 text-sm font-medium">Preset sekali-klik</p>
            <div className="flex flex-wrap gap-2">
              {Object.entries(AI_PROVIDER_PRESETS).map(([key, preset]) => (
                <button
                  key={key}
                  type="button"
                  onClick={() => applyPreset(key)}
                  className={`rounded-full px-3 py-1.5 text-sm font-semibold ${
                    presetKey === key ? 'bg-primary text-white' : 'bg-ink/10 dark:bg-darktext/10'
                  }`}
                >
                  {preset.label}
                </button>
              ))}
            </div>
          </div>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Nama provider</span>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="mis. OpenAI Utama"
              className={inputCls}
            />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Tipe provider</span>
            <select
              value={providerType}
              onChange={(e) => setProviderType(e.target.value as AiProviderType)}
              className={inputCls}
            >
              <option value="openai_compatible">OpenAI-compatible</option>
              <option value="anthropic">Anthropic</option>
              <option value="gemini">Gemini</option>
            </select>
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Base URL</span>
            <input
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://api.openai.com/v1"
              inputMode="url"
              className={inputCls}
            />
            <span className="mt-1 block text-xs text-ink/50 dark:text-darktext/50">
              Base URL bisa diubah manual setelah memilih preset.
            </span>
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Model</span>
            <input
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="gpt-4o-mini"
              className={inputCls}
            />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">
              API key {editing ? '(kosongkan bila tidak diubah)' : ''}
            </span>
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-… / kunci provider"
              autoComplete="off"
              className={inputCls}
            />
          </label>
          <label className="flex items-center justify-between gap-3">
            <span className="text-sm font-medium">Provider aktif</span>
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
              className="h-5 w-5 accent-primary"
            />
          </label>
          <button
            type="button"
            disabled={!formValid || saveMutation.isPending}
            onClick={() => saveMutation.mutate()}
            className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
          >
            {saveMutation.isPending ? 'Menyimpan…' : editing ? 'Simpan Perubahan' : 'Tambah Provider'}
          </button>
        </div>
      </Modal>

      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && deleteMutation.mutate(deleting.id)}
        title="Hapus AI Provider"
        message={`Hapus provider "${deleting?.name ?? ''}"? Bot yang memakai provider ini tidak akan bisa membalas otomatis lagi.`}
        busy={deleteMutation.isPending}
      />
    </Card>
  )
}
