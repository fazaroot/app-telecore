import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { History, Plus, Trash2, Workflow as WorkflowIcon } from 'lucide-react'
import { fetchBots } from '../api/bots'
import {
  createWorkflow,
  deleteWorkflow,
  fetchWorkflowExecutions,
  fetchWorkflows,
  toggleWorkflow,
  updateWorkflow,
} from '../api/automation'
import { useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { ConfirmDialog, Modal } from '../components/Modal'
import { WorkflowBuilder } from '../components/WorkflowBuilder'
import type { Workflow, WorkflowAction, WorkflowCondition } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

export function AutomationPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<Workflow | null>(null)
  const [deleting, setDeleting] = useState<Workflow | null>(null)
  const [historyFor, setHistoryFor] = useState<Workflow | null>(null)
  const [filterBot, setFilterBot] = useState('')

  const [wfBotId, setWfBotId] = useState('')
  const [wfName, setWfName] = useState('')
  const [triggerType, setTriggerType] = useState('message_received')
  const [triggerValue, setTriggerValue] = useState('')
  const [conditions, setConditions] = useState<WorkflowCondition[]>([])
  const [actions, setActions] = useState<WorkflowAction[]>([])

  const botsQuery = useQuery({ queryKey: ['bots'], queryFn: fetchBots })
  const workflowsQuery = useQuery({
    queryKey: ['workflows', filterBot],
    queryFn: () => fetchWorkflows(filterBot || undefined),
  })
  const executionsQuery = useQuery({
    queryKey: ['executions', historyFor?.id],
    queryFn: () => fetchWorkflowExecutions(historyFor!.id),
    enabled: historyFor !== null,
  })

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ['workflows'] })

  const saveMutation = useMutation({
    mutationFn: () => {
      // Engine backend membaca trigger_config.keywords (array) untuk trigger
      // keyword, dan trigger_config.command untuk trigger command.
      const triggerConfig =
        triggerType === 'keyword' && triggerValue.trim()
          ? { keywords: [triggerValue.trim()] }
          : triggerType === 'command' && triggerValue.trim()
            ? { command: triggerValue.trim() }
            : undefined
      const payload = {
        bot_id: wfBotId || undefined,
        name: wfName.trim(),
        trigger_type: triggerType,
        trigger_config: triggerConfig,
        conditions,
        actions,
        is_enabled: editing?.is_enabled ?? true,
      }
      return editing ? updateWorkflow(editing.id, payload) : createWorkflow(payload)
    },
    onSuccess: () => {
      pushToast(editing ? 'Workflow diperbarui.' : 'Workflow dibuat.', 'success')
      setShowForm(false)
      setEditing(null)
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const toggleMutation = useMutation({
    mutationFn: (id: string | number) => toggleWorkflow(id),
    onSuccess: () => {
      pushToast('Status workflow diubah.', 'success')
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteMutation = useMutation({
    mutationFn: (id: string | number) => deleteWorkflow(id),
    onSuccess: () => {
      pushToast('Workflow dihapus.', 'success')
      setDeleting(null)
      invalidate()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const openForm = (wf?: Workflow) => {
    setEditing(wf ?? null)
    setWfBotId(wf?.bot_id !== undefined ? String(wf.bot_id) : '')
    setWfName(wf?.name ?? '')
    setTriggerType(wf?.trigger_type ?? 'message_received')
    setTriggerValue(
      String(
        (wf?.trigger_config?.keywords as string[] | undefined)?.[0] ??
          wf?.trigger_config?.command ??
          wf?.trigger_config?.value ??
          '',
      ),
    )
    setConditions(wf?.conditions ?? [])
    setActions(wf?.actions ?? [])
    setShowForm(true)
  }

  const bots = botsQuery.data ?? []
  const workflows = workflowsQuery.data ?? []
  const executions = executionsQuery.data ?? []

  if (workflowsQuery.isLoading) return <LoadingSpinner label="Memuat workflow…" />
  if (workflowsQuery.isError) {
    return (
      <div>
        <PageHeader title="Automation" subtitle="Workflow otomatis untuk bot Anda" />
        <ErrorState message={(workflowsQuery.error as Error).message} onRetry={() => void workflowsQuery.refetch()} />
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <PageHeader
        title="Automation"
        subtitle="Workflow otomatis untuk bot Anda"
        action={
          <button
            type="button"
            onClick={() => openForm()}
            className="flex items-center gap-1 rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
          >
            <Plus className="h-4 w-4" /> Workflow
          </button>
        }
      />
      <p className="-mt-2 text-xs text-ink/50 dark:text-darktext/50">
        Builder berbasis langkah: pilih trigger, tambah kondisi, lalu tambah aksi. Bukan drag-drop.
      </p>

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

      {workflows.length === 0 ? (
        <EmptyState
          icon={<WorkflowIcon className="h-10 w-10 text-primary" />}
          title="Belum ada workflow"
          description="Buat workflow pertama: mis. balas otomatis saat ada kata kunci tertentu."
          action={
            <button
              type="button"
              onClick={() => openForm()}
              className="mt-2 rounded-full bg-primary px-4 py-2 text-sm font-semibold text-white"
            >
              Buat Workflow
            </button>
          }
        />
      ) : (
        workflows.map((wf) => (
          <Card key={String(wf.id)}>
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <p className="font-bold">{wf.name ?? 'Workflow tanpa nama'}</p>
                <p className="text-sm text-ink/60 dark:text-darktext/60">
                  Trigger: {wf.trigger_type ?? '—'} · {wf.conditions?.length ?? 0} kondisi · {wf.actions?.length ?? 0} aksi
                </p>
              </div>
              <Badge tone={wf.is_enabled ? 'success' : 'default'}>{wf.is_enabled ? 'Aktif' : 'Nonaktif'}</Badge>
            </div>
            <div className="mt-3 flex flex-wrap gap-2">
              <button
                type="button"
                onClick={() => toggleMutation.mutate(wf.id)}
                disabled={toggleMutation.isPending}
                className="rounded-full bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary"
              >
                {wf.is_enabled ? 'Nonaktifkan' : 'Aktifkan'}
              </button>
              <button
                type="button"
                onClick={() => openForm(wf)}
                className="rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
              >
                Edit
              </button>
              <button
                type="button"
                onClick={() => setHistoryFor(wf)}
                className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
              >
                <History className="h-3.5 w-3.5" /> Riwayat
              </button>
              <button
                type="button"
                onClick={() => setDeleting(wf)}
                className="flex items-center gap-1 rounded-full bg-danger/10 px-3 py-1.5 text-xs font-semibold text-danger"
              >
                <Trash2 className="h-3.5 w-3.5" /> Hapus
              </button>
            </div>
          </Card>
        ))
      )}

      <Modal open={showForm} onClose={() => setShowForm(false)} title={editing ? 'Edit Workflow' : 'Workflow Baru'}>
        <div className="space-y-4">
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Bot</span>
            <select value={wfBotId} onChange={(e) => setWfBotId(e.target.value)} className={inputCls}>
              <option value="">Pilih bot…</option>
              {bots.map((b) => (
                <option key={String(b.id)} value={String(b.id)}>
                  {b.name ?? b.username ?? `Bot ${b.id}`}
                </option>
              ))}
            </select>
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Nama workflow</span>
            <input value={wfName} onChange={(e) => setWfName(e.target.value)} placeholder="mis. Balas kata 'harga'" className={inputCls} />
          </label>
          <WorkflowBuilder
            triggerType={triggerType}
            triggerValue={triggerValue}
            conditions={conditions}
            actions={actions}
            onTriggerTypeChange={setTriggerType}
            onTriggerValueChange={setTriggerValue}
            onConditionsChange={setConditions}
            onActionsChange={setActions}
          />
          <button
            type="button"
            disabled={!wfBotId || !wfName.trim() || actions.length === 0 || saveMutation.isPending}
            onClick={() => saveMutation.mutate()}
            className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
          >
            {saveMutation.isPending ? 'Menyimpan…' : 'Simpan Workflow'}
          </button>
        </div>
      </Modal>

      <Modal open={historyFor !== null} onClose={() => setHistoryFor(null)} title={`Riwayat — ${historyFor?.name ?? ''}`}>
        {executionsQuery.isLoading ? (
          <LoadingSpinner label="Memuat riwayat eksekusi…" />
        ) : executionsQuery.isError ? (
          <ErrorState message={(executionsQuery.error as Error).message} onRetry={() => void executionsQuery.refetch()} />
        ) : executions.length === 0 ? (
          <EmptyState title="Belum ada eksekusi" description="Workflow ini belum pernah berjalan." />
        ) : (
          <ul className="space-y-2">
            {executions.map((ex) => (
              <li key={String(ex.id)} className="rounded-xl border border-ink/10 p-3 text-sm dark:border-darktext/10">
                <div className="flex items-center justify-between gap-2">
                  <Badge tone={ex.status === 'failed' || ex.status === 'error' ? 'danger' : 'success'}>
                    {ex.status ?? '—'}
                  </Badge>
                  <span className="text-xs text-ink/50 dark:text-darktext/50">
                    {(ex.started_at ?? ex.created_at)
                      ? new Date((ex.started_at ?? ex.created_at) as string).toLocaleString('id-ID')
                      : ''}
                  </span>
                </div>
                {(ex.error ?? ex.error_message) && (
                  <p className="mt-1 text-xs text-danger">{ex.error ?? ex.error_message}</p>
                )}
                {ex.steps_executed !== undefined && (
                  <p className="mt-1 text-xs text-ink/50 dark:text-darktext/50">
                    {ex.steps_executed} langkah
                    {ex.duration_ms !== undefined ? ` · ${ex.duration_ms} ms` : ''}
                  </p>
                )}
              </li>
            ))}
          </ul>
        )}
      </Modal>

      <ConfirmDialog
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        onConfirm={() => deleting && deleteMutation.mutate(deleting.id)}
        title="Hapus Workflow"
        message={`Hapus workflow "${deleting?.name ?? ''}"? Riwayat eksekusinya ikut terhapus.`}
        busy={deleteMutation.isPending}
      />
    </div>
  )
}
