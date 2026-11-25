import { useEffect, useState } from 'react'
import {
  Bell,
  ChevronDown,
  ChevronUp,
  Clock,
  FileText,
  Filter,
  Play,
  Plus,
  Save,
  Send,
  Tag,
  Trash2,
  Zap,
} from 'lucide-react'
import type { WorkflowAction } from '../types'
import {
  ACTION_TYPES,
  CONDITION_FIELDS,
  CONDITION_OPERATORS,
  TEMPLATE_VARIABLES,
  TRIGGER_TYPES,
  actionLabel,
  actionSummary,
  conditionSummary,
  createActionNode,
  createConditionNode,
  moveActionNode,
  triggerLabel,
  triggerSummary,
  type ActionNode,
  type BuilderState,
  type ConditionNode,
} from './workflowNodes'

export { ACTION_TYPES, CONDITION_FIELDS, CONDITION_OPERATORS, TRIGGER_TYPES }

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

function Connector() {
  return <div aria-hidden="true" className="mx-auto h-6 w-px bg-ink/20 dark:bg-darktext/20" />
}

function actionIcon(actionType?: string) {
  switch (actionType) {
    case 'send_message':
    case 'reply':
      return <Send className="h-4 w-4 text-primary" />
    case 'save_activity':
      return <Save className="h-4 w-4 text-primary" />
    case 'notify_admin':
      return <Bell className="h-4 w-4 text-primary" />
    case 'send_template':
      return <FileText className="h-4 w-4 text-primary" />
    case 'add_label':
      return <Tag className="h-4 w-4 text-primary" />
    case 'save_note':
      return <FileText className="h-4 w-4 text-primary" />
    case 'delay':
      return <Clock className="h-4 w-4 text-primary" />
    default:
      return <Play className="h-4 w-4 text-primary" />
  }
}

function nodeCardCls(selected: boolean): string {
  return `flex items-stretch gap-2 rounded-xl border bg-surface p-3 dark:bg-darksurface ${
    selected ? 'border-primary ring-2 ring-primary/30' : 'border-ink/10 dark:border-darktext/10'
  }`
}

/**
 * Visual Automation Builder: kanvas node vertikal
 * (Trigger → Condition → Action berurutan) dengan garis penghubung.
 * Klik node membuka panel Properties untuk mengedit node itu.
 * Bukan drag-drop — urutan action diubah dengan tombol naik/turun.
 */
export function WorkflowBuilder({
  state,
  onChange,
}: {
  state: BuilderState
  onChange: (state: BuilderState) => void
}) {
  const [selectedId, setSelectedId] = useState(state.trigger.id)

  // Workflow lain dibuka -> kembali memilih node Trigger.
  useEffect(() => {
    setSelectedId(state.trigger.id)
  }, [state.trigger.id])

  const selectedCondition = state.conditions.find((c) => c.id === selectedId)
  const selectedAction = state.actions.find((a) => a.id === selectedId)
  const selectedActionIndex = state.actions.findIndex((a) => a.id === selectedId)
  const showTriggerProperties = !selectedCondition && !selectedAction

  const updateTrigger = (patch: Partial<BuilderState['trigger']>) =>
    onChange({ ...state, trigger: { ...state.trigger, ...patch } })

  const updateCondition = (id: string, patch: Partial<ConditionNode>) =>
    onChange({
      ...state,
      conditions: state.conditions.map((c) => (c.id === id ? { ...c, ...patch } : c)),
    })

  const updateAction = (id: string, patch: Partial<WorkflowAction>) =>
    onChange({
      ...state,
      actions: state.actions.map((a) => (a.id === id ? { ...a, action: { ...a.action, ...patch } } : a)),
    })

  const addCondition = () => {
    const node = createConditionNode()
    onChange({ ...state, conditions: [...state.conditions, node] })
    setSelectedId(node.id)
  }

  const addAction = () => {
    const node = createActionNode('send_message', { text: '' })
    onChange({ ...state, actions: [...state.actions, node] })
    setSelectedId(node.id)
  }

  const removeCondition = (id: string) => {
    onChange({ ...state, conditions: state.conditions.filter((c) => c.id !== id) })
    setSelectedId(state.trigger.id)
  }

  const removeAction = (id: string) => {
    onChange({ ...state, actions: state.actions.filter((a) => a.id !== id) })
    setSelectedId(state.trigger.id)
  }

  const renderConditionCard = (condition: ConditionNode) => (
    <div key={condition.id} className={nodeCardCls(selectedId === condition.id)}>
      <button
        type="button"
        onClick={() => setSelectedId(condition.id)}
        className="min-w-0 flex-1 text-left"
        aria-label={`Condition: ${conditionSummary(condition)}`}
      >
        <span className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-ink/50 dark:text-darktext/50">
          <Filter className="h-4 w-4 text-primary" /> Condition
        </span>
        <span className="mt-1 block break-words text-sm font-bold [overflow-wrap:anywhere]">
          {conditionSummary(condition)}
        </span>
      </button>
      <button
        type="button"
        aria-label="Hapus condition"
        onClick={() => removeCondition(condition.id)}
        className="self-start rounded-full p-1.5 text-danger hover:bg-danger/10"
      >
        <Trash2 className="h-4 w-4" />
      </button>
    </div>
  )

  const renderActionCard = (node: ActionNode, index: number) => (
    <div key={node.id} className={nodeCardCls(selectedId === node.id)}>
      <button
        type="button"
        onClick={() => setSelectedId(node.id)}
        className="min-w-0 flex-1 text-left"
        aria-label={`Action ${index + 1}: ${actionLabel(node.action.type)}`}
      >
        <span className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-ink/50 dark:text-darktext/50">
          {actionIcon(node.action.type)} Action {index + 1}
        </span>
        <span className="mt-1 block break-words text-sm font-bold [overflow-wrap:anywhere]">
          {actionLabel(node.action.type)}
        </span>
        <span className="mt-0.5 block break-words text-sm text-ink/60 [overflow-wrap:anywhere] dark:text-darktext/60">
          {actionSummary(node.action)}
        </span>
      </button>
      <span className="flex shrink-0 flex-col gap-1">
        <button
          type="button"
          aria-label="Pindah action ke atas"
          disabled={index === 0}
          onClick={() => onChange(moveActionNode(state, index, -1))}
          className="rounded-full p-1.5 hover:bg-ink/10 disabled:opacity-30 dark:hover:bg-darktext/10"
        >
          <ChevronUp className="h-4 w-4" />
        </button>
        <button
          type="button"
          aria-label="Pindah action ke bawah"
          disabled={index === state.actions.length - 1}
          onClick={() => onChange(moveActionNode(state, index, 1))}
          className="rounded-full p-1.5 hover:bg-ink/10 disabled:opacity-30 dark:hover:bg-darktext/10"
        >
          <ChevronDown className="h-4 w-4" />
        </button>
        <button
          type="button"
          aria-label="Hapus action"
          onClick={() => removeAction(node.id)}
          className="rounded-full p-1.5 text-danger hover:bg-danger/10"
        >
          <Trash2 className="h-4 w-4" />
        </button>
      </span>
    </div>
  )

  return (
    <div className="space-y-4">
      {/* Kanvas node vertikal (bisa di-scroll, tanpa overflow menyamping) */}
      <div
        aria-label="Kanvas workflow"
        className="max-h-[45vh] overflow-x-hidden overflow-y-auto rounded-2xl border border-ink/10 bg-bg p-3 dark:border-darktext/10 dark:bg-darkbg"
      >
        <div className={nodeCardCls(showTriggerProperties)}>
          <button
            type="button"
            onClick={() => setSelectedId(state.trigger.id)}
            className="min-w-0 flex-1 text-left"
            aria-label={`Trigger: ${triggerSummary(state.trigger)}`}
          >
            <span className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-ink/50 dark:text-darktext/50">
              <Zap className="h-4 w-4 text-primary" /> Trigger
            </span>
            <span className="mt-1 block break-words text-sm font-bold [overflow-wrap:anywhere]">
              {triggerLabel(state.trigger.triggerType)}
            </span>
            <span className="mt-0.5 block break-words text-sm text-ink/60 [overflow-wrap:anywhere] dark:text-darktext/60">
              {triggerSummary(state.trigger)}
            </span>
          </button>
        </div>

        <Connector />

        {state.conditions.length > 1 && (
          <label className="mb-3 block text-sm">
            <span className="mb-1 block font-medium">Logika condition</span>
            <select
              value={state.conditionLogic}
              onChange={(e) => onChange({ ...state, conditionLogic: e.target.value === 'OR' ? 'OR' : 'AND' })}
              className={inputCls}
            >
              <option value="AND">Semua condition harus cocok (AND)</option>
              <option value="OR">Salah satu condition cocok (OR)</option>
            </select>
          </label>
        )}

        {state.conditions.map((condition) => (
          <div key={condition.id}>
            {renderConditionCard(condition)}
            <Connector />
          </div>
        ))}

        <button
          type="button"
          onClick={addCondition}
          className="flex w-full items-center justify-center gap-1 rounded-xl border border-dashed border-primary/40 px-3 py-2 text-sm font-semibold text-primary"
        >
          <Plus className="h-4 w-4" /> Tambah Condition
        </button>

        <Connector />

        {state.actions.length === 0 && (
          <p className="mb-2 text-sm text-ink/60 dark:text-darktext/60">
            Workflow membutuhkan minimal satu action.
          </p>
        )}
        {state.actions.map((node, index) => (
          <div key={node.id}>
            {renderActionCard(node, index)}
            <Connector />
          </div>
        ))}

        <button
          type="button"
          onClick={addAction}
          className="flex w-full items-center justify-center gap-1 rounded-xl border border-dashed border-primary/40 px-3 py-2 text-sm font-semibold text-primary"
        >
          <Plus className="h-4 w-4" /> Tambah Action
        </button>
      </div>

      {/* Panel Properties untuk node terpilih */}
      <div className="rounded-2xl border border-ink/10 p-4 dark:border-darktext/10">
        <h3 className="mb-3 break-words text-sm font-bold [overflow-wrap:anywhere]">
          Properties —{' '}
          {selectedAction
            ? `Action ${selectedActionIndex + 1}: ${actionLabel(selectedAction.action.type)}`
            : selectedCondition
              ? `Condition: ${conditionSummary(selectedCondition)}`
              : `Trigger: ${triggerLabel(state.trigger.triggerType)}`}
        </h3>

        {showTriggerProperties && (
          <div className="space-y-3">
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Tipe trigger</span>
              <select
                value={state.trigger.triggerType}
                onChange={(e) => updateTrigger({ triggerType: e.target.value })}
                className={inputCls}
              >
                {TRIGGER_TYPES.map((t) => (
                  <option key={t.value} value={t.value}>
                    {t.label}
                  </option>
                ))}
                {!TRIGGER_TYPES.some((t) => t.value === state.trigger.triggerType) && (
                  <option value={state.trigger.triggerType}>{state.trigger.triggerType} (lama)</option>
                )}
              </select>
            </label>
            {state.trigger.triggerType === 'keyword' && (
              <label className="block text-sm">
                <span className="mb-1 block font-medium">Kata kunci pemicu</span>
                <input
                  value={state.trigger.triggerValue}
                  onChange={(e) => updateTrigger({ triggerValue: e.target.value })}
                  placeholder='mis. "harga"'
                  className={inputCls}
                />
              </label>
            )}
            {state.trigger.triggerType === 'command' && (
              <label className="block text-sm">
                <span className="mb-1 block font-medium">Command pemicu</span>
                <input
                  value={state.trigger.triggerValue}
                  onChange={(e) => updateTrigger({ triggerValue: e.target.value })}
                  placeholder="mis. /start"
                  className={inputCls}
                />
              </label>
            )}
            {state.trigger.triggerType === 'message_received' && (
              <p className="text-sm text-ink/60 dark:text-darktext/60">
                Workflow berjalan setiap pesan masuk. Tambahkan Condition bila hanya pesan tertentu yang
                harus diproses.
              </p>
            )}
            {state.trigger.triggerType === 'scheduled' && (
              <p className="text-sm text-ink/60 dark:text-darktext/60">
                Workflow berjalan mengikuti jadwal dari Scheduler. Konfigurasi jadwal lama dipertahankan.
              </p>
            )}
          </div>
        )}

        {selectedCondition && (
          <div className="space-y-3">
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Field</span>
              <select
                value={selectedCondition.field}
                onChange={(e) => updateCondition(selectedCondition.id, { field: e.target.value })}
                className={inputCls}
              >
                {CONDITION_FIELDS.map((f) => (
                  <option key={f.value} value={f.value}>
                    {f.label}
                  </option>
                ))}
                {!CONDITION_FIELDS.some((f) => f.value === selectedCondition.field) && (
                  <option value={selectedCondition.field}>{selectedCondition.field} (lama)</option>
                )}
              </select>
            </label>
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Operator</span>
              <select
                value={selectedCondition.operator}
                onChange={(e) => updateCondition(selectedCondition.id, { operator: e.target.value })}
                className={inputCls}
              >
                {CONDITION_OPERATORS.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
                {!CONDITION_OPERATORS.some((o) => o.value === selectedCondition.operator) && (
                  <option value={selectedCondition.operator}>{selectedCondition.operator} (lama)</option>
                )}
              </select>
            </label>
            <label className="block text-sm">
              <span className="mb-1 block font-medium">
                {selectedCondition.field === 'text' && selectedCondition.operator === 'contains'
                  ? 'Kata kunci'
                  : 'Nilai'}
              </span>
              <input
                value={selectedCondition.value}
                onChange={(e) => updateCondition(selectedCondition.id, { value: e.target.value })}
                placeholder={
                  selectedCondition.field === 'text' ? 'mis. harga' : 'Nilai pembanding'
                }
                className={inputCls}
              />
            </label>
            <button
              type="button"
              onClick={() => removeCondition(selectedCondition.id)}
              className="flex items-center gap-1 rounded-full bg-danger/10 px-3 py-1.5 text-xs font-semibold text-danger"
            >
              <Trash2 className="h-3.5 w-3.5" /> Hapus condition
            </button>
          </div>
        )}

        {selectedAction && (
          <div className="space-y-3">
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Tipe action</span>
              <select
                value={selectedAction.action.type ?? 'send_message'}
                onChange={(e) => {
                  const type = e.target.value
                  updateAction(selectedAction.id, {
                    type,
                    ...(type === 'delay' && selectedAction.action.seconds === undefined
                      ? { seconds: 1 }
                      : {}),
                  })
                }}
                className={inputCls}
              >
                {ACTION_TYPES.map((t) => (
                  <option key={t.value} value={t.value}>
                    {t.label}
                  </option>
                ))}
                {!ACTION_TYPES.some((t) => t.value === selectedAction.action.type) && (
                  <option value={selectedAction.action.type ?? ''}>
                    {selectedAction.action.type} (lama)
                  </option>
                )}
              </select>
            </label>

            {(selectedAction.action.type === 'send_message' || selectedAction.action.type === 'reply') && (
              <>
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Teks balasan</span>
                  <textarea
                    value={String(selectedAction.action.text ?? '')}
                    onChange={(e) => updateAction(selectedAction.id, { text: e.target.value })}
                    placeholder="Halo {{first_name}}, terima kasih sudah menghubungi kami."
                    rows={3}
                    className={inputCls}
                  />
                </label>
                <p className="break-words text-xs text-ink/50 [overflow-wrap:anywhere] dark:text-darktext/50">
                  Variabel template: {TEMPLATE_VARIABLES.join(' ')}
                </p>
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Chat ID tujuan (opsional)</span>
                  <input
                    value={String(selectedAction.action.chat_id ?? '')}
                    onChange={(e) => updateAction(selectedAction.id, { chat_id: e.target.value })}
                    placeholder="Kosongkan untuk chat asal pesan"
                    inputMode="numeric"
                    className={inputCls}
                  />
                </label>
              </>
            )}

            {selectedAction.action.type === 'save_activity' && (
              <>
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Teks aktivitas</span>
                  <textarea
                    value={String(selectedAction.action.text ?? '')}
                    onChange={(e) => updateAction(selectedAction.id, { text: e.target.value })}
                    placeholder="mis. Pengguna menanyakan harga"
                    rows={2}
                    className={inputCls}
                  />
                  <span className="mt-1 block break-words text-xs text-ink/50 [overflow-wrap:anywhere] dark:text-darktext/50">
                    Aktivitas dicatat ke log. Variabel template: {TEMPLATE_VARIABLES.join(' ')}
                  </span>
                </label>
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Label aktivitas (opsional)</span>
                  <input
                    value={String(selectedAction.action.label ?? '')}
                    onChange={(e) => updateAction(selectedAction.id, { label: e.target.value })}
                    placeholder="mis. harga"
                    className={inputCls}
                  />
                </label>
              </>
            )}

            {selectedAction.action.type === 'notify_admin' && (
              <>
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Teks notifikasi</span>
                  <textarea
                    value={String(selectedAction.action.text ?? '')}
                    onChange={(e) => updateAction(selectedAction.id, { text: e.target.value })}
                    placeholder="Ada pesan baru dari {{first_name}}: {{text}}"
                    rows={3}
                    className={inputCls}
                  />
                </label>
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Admin chat ID (opsional)</span>
                  <input
                    value={String(selectedAction.action.admin_chat_id ?? '')}
                    onChange={(e) => updateAction(selectedAction.id, { admin_chat_id: e.target.value })}
                    placeholder="Kosongkan bila tidak menentukan chat admin khusus"
                    inputMode="numeric"
                    className={inputCls}
                  />
                </label>
              </>
            )}

            {selectedAction.action.type === 'send_template' && (
              <label className="block text-sm">
                <span className="mb-1 block font-medium">Template ID</span>
                <input
                  value={String(selectedAction.action.template_id ?? '')}
                  onChange={(e) => updateAction(selectedAction.id, { template_id: e.target.value })}
                  placeholder="ID template pesan di server"
                  className={inputCls}
                />
              </label>
            )}

            {selectedAction.action.type === 'add_label' && (
              <label className="block text-sm">
                <span className="mb-1 block font-medium">Nama label</span>
                <input
                  value={String(selectedAction.action.label ?? '')}
                  onChange={(e) => updateAction(selectedAction.id, { label: e.target.value })}
                  placeholder="mis. calon-pembeli"
                  className={inputCls}
                />
              </label>
            )}

            {selectedAction.action.type === 'save_note' && (
              <label className="block text-sm">
                <span className="mb-1 block font-medium">Isi catatan</span>
                <textarea
                  value={String(selectedAction.action.note ?? '')}
                  onChange={(e) => updateAction(selectedAction.id, { note: e.target.value })}
                  placeholder="Catatan yang disimpan ke profil pengguna"
                  rows={2}
                  className={inputCls}
                />
              </label>
            )}

            {selectedAction.action.type === 'delay' && (
              <label className="block text-sm">
                <span className="mb-1 block font-medium">Durasi tunda (detik, maks 30)</span>
                <input
                  type="number"
                  min={0}
                  max={30}
                  value={Number(selectedAction.action.seconds ?? selectedAction.action.delay_seconds ?? 1)}
                  onChange={(e) =>
                    updateAction(selectedAction.id, {
                      seconds: Math.min(30, Math.max(0, Number(e.target.value) || 0)),
                    })
                  }
                  className={inputCls}
                />
              </label>
            )}

            {!ACTION_TYPES.some((t) => t.value === selectedAction.action.type) && (
              <div className="space-y-3">
                <p className="rounded-xl bg-amber-500/15 p-3 text-sm text-amber-700 dark:text-amber-400">
                  Tipe action lama ini dipertahankan apa adanya agar data workflow tidak hilang. Backend
                  mungkin tidak mendukungnya lagi.
                </p>
                {'text' in selectedAction.action && (
                  <label className="block text-sm">
                    <span className="mb-1 block font-medium">Teks</span>
                    <textarea
                      value={String(selectedAction.action.text ?? '')}
                      onChange={(e) => updateAction(selectedAction.id, { text: e.target.value })}
                      rows={2}
                      className={inputCls}
                    />
                  </label>
                )}
              </div>
            )}

            {selectedAction.action.type !== 'delay' && selectedAction.action.delay_seconds !== undefined && (
              <label className="block text-sm">
                <span className="mb-1 block font-medium">Delay sebelum aksi ini (detik, data lama)</span>
                <input
                  type="number"
                  min={0}
                  value={Number(selectedAction.action.delay_seconds ?? 0)}
                  onChange={(e) =>
                    updateAction(selectedAction.id, {
                      delay_seconds: Math.max(0, Number(e.target.value) || 0),
                    })
                  }
                  className={inputCls}
                />
              </label>
            )}

            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                disabled={selectedActionIndex <= 0}
                onClick={() => onChange(moveActionNode(state, selectedActionIndex, -1))}
                className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold disabled:opacity-40 dark:bg-darktext/10"
              >
                <ChevronUp className="h-3.5 w-3.5" /> Pindah ke atas
              </button>
              <button
                type="button"
                disabled={selectedActionIndex >= state.actions.length - 1}
                onClick={() => onChange(moveActionNode(state, selectedActionIndex, 1))}
                className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold disabled:opacity-40 dark:bg-darktext/10"
              >
                <ChevronDown className="h-3.5 w-3.5" /> Pindah ke bawah
              </button>
              <button
                type="button"
                onClick={() => removeAction(selectedAction.id)}
                className="flex items-center gap-1 rounded-full bg-danger/10 px-3 py-1.5 text-xs font-semibold text-danger"
              >
                <Trash2 className="h-3.5 w-3.5" /> Hapus action
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
