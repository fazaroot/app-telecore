import type { Workflow, WorkflowAction, WorkflowCondition, WorkflowConditions } from '../types'

/**
 * Model node Visual Automation Builder + pemetaan murni node <-> payload API.
 *
 * Payload yang dikirim ke backend tetap bentuk lama:
 * `{ bot_id, name, trigger_type, trigger_config, conditions, actions, is_enabled }`.
 *
 * Kontrak backend (engine.rs):
 * - Trigger: message_received, keyword, command, new_user, scheduled,
 *   user_joined_group. Keyword membaca `trigger_config.keywords` (array),
 *   command membaca `trigger_config.command`.
 * - Condition: array rules (AND) atau `{ logic, rules }`; rule =
 *   `{ field, operator, value }` dengan field konteks event (text, chat_id,
 *   chat_type, user_id, username, first_name, message_id).
 * - Action lama: send_message `{text}`, send_template `{template_id}`,
 *   add_label `{label}`, save_note `{note}`, delay `{seconds}`.
 * - Action v1.4: save_activity `{text, label?}`, notify_admin
 *   `{text, admin_chat_id?}`.
 *
 * Node menyimpan objek action/condition aslinya (spread) sehingga key lama
 * yang tidak dikenal UI (mis. `delay_seconds` per aksi dari builder v1.3)
 * tidak hilang saat workflow lama dibuka dan disimpan ulang.
 */

export const TRIGGER_TYPES = [
  { value: 'message_received', label: 'Pesan masuk' },
  { value: 'keyword', label: 'Kata kunci cocok' },
  { value: 'command', label: 'Command diterima' },
  { value: 'new_user', label: 'Pengguna baru' },
  { value: 'user_joined_group', label: 'Pengguna bergabung ke grup' },
  { value: 'scheduled', label: 'Jadwal (scheduled)' },
]

export const ACTION_TYPES = [
  { value: 'send_message', label: 'Kirim balasan' },
  { value: 'save_activity', label: 'Simpan aktivitas' },
  { value: 'notify_admin', label: 'Kirim notifikasi admin' },
  { value: 'send_template', label: 'Kirim template' },
  { value: 'add_label', label: 'Tambah label pengguna' },
  { value: 'save_note', label: 'Simpan catatan pengguna' },
  { value: 'delay', label: 'Tunda (delay)' },
]

export const CONDITION_FIELDS = [
  { value: 'text', label: 'Isi pesan' },
  { value: 'chat_type', label: 'Tipe chat' },
  { value: 'chat_id', label: 'Chat ID' },
  { value: 'user_id', label: 'User ID' },
  { value: 'username', label: 'Username' },
  { value: 'first_name', label: 'Nama depan' },
  { value: 'message_id', label: 'Message ID' },
]

export const CONDITION_OPERATORS = [
  { value: 'contains', label: 'mengandung' },
  { value: 'not_contains', label: 'tidak mengandung' },
  { value: 'equals', label: 'sama dengan' },
  { value: 'not_equals', label: 'tidak sama dengan' },
  { value: 'starts_with', label: 'diawali' },
  { value: 'ends_with', label: 'diakhiri' },
  { value: 'gt', label: 'lebih besar dari' },
  { value: 'lt', label: 'lebih kecil dari' },
]

/** Variabel template yang bisa dipakai di teks balasan/notifikasi. */
export const TEMPLATE_VARIABLES = ['{{first_name}}', '{{username}}', '{{text}}', '{{chat_id}}', '{{user_id}}']

const NUMERIC_CONDITION_FIELDS = new Set(['chat_id', 'user_id', 'message_id'])

let nodeSeq = 0
function nextNodeId(prefix: string): string {
  nodeSeq += 1
  return `${prefix}-${nodeSeq}`
}

export interface TriggerNode {
  id: string
  kind: 'trigger'
  triggerType: string
  triggerValue: string
  rawConfig: Record<string, unknown>
}

export interface ConditionNode {
  id: string
  kind: 'condition'
  field: string
  operator: string
  value: string
  raw: WorkflowCondition
}

export interface ActionNode {
  id: string
  kind: 'action'
  action: WorkflowAction
}

export type BuilderNode = TriggerNode | ConditionNode | ActionNode

export interface BuilderState {
  trigger: TriggerNode
  conditions: ConditionNode[]
  actions: ActionNode[]
  conditionLogic: 'AND' | 'OR'
}

export interface WorkflowPayload {
  bot_id: string
  name: string
  trigger_type: string
  trigger_config: Record<string, unknown>
  conditions: WorkflowConditions
  actions: WorkflowAction[]
  is_enabled: boolean
}

export function createTriggerNode(triggerType = 'message_received'): TriggerNode {
  return { id: nextNodeId('trigger'), kind: 'trigger', triggerType, triggerValue: '', rawConfig: {} }
}

export function createConditionNode(partial: Partial<WorkflowCondition> = {}): ConditionNode {
  const field = partial.field ?? 'text'
  const operator = normalizeOperator(partial.operator ?? 'contains')
  const value = partial.value === undefined || partial.value === null ? '' : String(partial.value)
  return {
    id: nextNodeId('condition'),
    kind: 'condition',
    field,
    operator,
    value,
    raw: { ...partial, field, operator, value: partial.value ?? '' },
  }
}

export function createActionNode(type = 'send_message', partial: WorkflowAction = {}): ActionNode {
  return { id: nextNodeId('action'), kind: 'action', action: defaultActionFields(type, partial) }
}

function defaultActionFields(type: string, partial: WorkflowAction): WorkflowAction {
  const base: WorkflowAction = { ...partial, type }
  if (type === 'delay' && base.seconds === undefined) {
    base.seconds = typeof base.delay_seconds === 'number' ? base.delay_seconds : 1
  }
  return base
}

export function defaultBuilderState(): BuilderState {
  return {
    trigger: createTriggerNode('message_received'),
    conditions: [],
    actions: [createActionNode('send_message', { text: '' })],
    conditionLogic: 'AND',
  }
}

function normalizeOperator(operator: string): string {
  switch (operator) {
    case 'eq':
      return 'equals'
    case 'ne':
      return 'not_equals'
    case 'greater_than':
      return 'gt'
    case 'less_than':
      return 'lt'
    default:
      return operator
  }
}

function normalizeTriggerType(triggerType: string): string {
  // Builder lama memakai 'schedule'; backend mengenalnya sebagai 'scheduled'.
  return triggerType === 'schedule' ? 'scheduled' : triggerType
}

function parseConditions(conditions: WorkflowConditions | undefined): {
  rules: WorkflowCondition[]
  logic: 'AND' | 'OR'
} {
  if (Array.isArray(conditions)) return { rules: conditions, logic: 'AND' }
  if (conditions && typeof conditions === 'object') {
    return {
      rules: Array.isArray(conditions.rules) ? conditions.rules : [],
      logic: String(conditions.logic ?? 'AND').toUpperCase() === 'OR' ? 'OR' : 'AND',
    }
  }
  return { rules: [], logic: 'AND' }
}

/** Petakan workflow dari API menjadi node builder. `undefined` = workflow baru. */
export function workflowToNodes(workflow?: Workflow | null): BuilderState {
  if (!workflow) return defaultBuilderState()

  const triggerType = normalizeTriggerType(workflow.trigger_type ?? 'message_received')
  const rawConfig: Record<string, unknown> = { ...(workflow.trigger_config ?? {}) }
  const keywords = Array.isArray(rawConfig.keywords)
    ? rawConfig.keywords.filter((k): k is string => typeof k === 'string')
    : []
  const triggerValue =
    triggerType === 'keyword'
      ? (keywords[0] ?? '')
      : triggerType === 'command'
        ? String(rawConfig.command ?? '')
        : String(rawConfig.value ?? '')

  const { rules, logic } = parseConditions(workflow.conditions)

  return {
    trigger: { id: nextNodeId('trigger'), kind: 'trigger', triggerType, triggerValue, rawConfig },
    conditions: rules.map((rule) => createConditionNode(rule)),
    actions: (workflow.actions ?? []).map((action) =>
      createActionNode(action.type ?? 'send_message', action),
    ),
    conditionLogic: logic,
  }
}

function conditionNodeToRule(node: ConditionNode): WorkflowCondition {
  const rawValue = node.value
  const numeric = NUMERIC_CONDITION_FIELDS.has(node.field) || node.operator === 'gt' || node.operator === 'lt'
  const parsed = Number(rawValue)
  const value: string | number =
    numeric && rawValue.trim() !== '' && Number.isFinite(parsed) ? parsed : rawValue
  return { ...node.raw, field: node.field, operator: node.operator, value }
}

function toNumberIfNumeric(value: unknown): number | string | undefined {
  if (value === undefined || value === null || value === '') return undefined
  if (typeof value === 'number') return value
  const parsed = Number(value)
  return Number.isFinite(parsed) ? parsed : String(value)
}

function actionNodeToPayload(action: WorkflowAction): WorkflowAction {
  const out: WorkflowAction = { ...action, type: action.type ?? 'send_message' }
  const type = out.type

  if (type === 'delay') {
    const seconds = Number(out.seconds ?? out.delay_seconds ?? 1)
    out.seconds = Number.isFinite(seconds) ? Math.min(30, Math.max(0, Math.trunc(seconds))) : 1
    delete out.delay_seconds
  }
  if (type === 'notify_admin') {
    const adminChatId = toNumberIfNumeric(out.admin_chat_id)
    if (adminChatId === undefined) delete out.admin_chat_id
    else out.admin_chat_id = adminChatId
  }
  if (type === 'send_message' || type === 'send_template') {
    const chatId = toNumberIfNumeric(out.chat_id)
    if (chatId === undefined) delete out.chat_id
    else out.chat_id = chatId
  }
  if (type === 'add_label' && typeof out.label === 'string') out.label = out.label.trim()
  if (type === 'send_template' && typeof out.template_id === 'string') {
    out.template_id = out.template_id.trim()
  }
  if (type === 'save_activity' && typeof out.label === 'string') {
    out.label = out.label.trim()
    if (!out.label) delete out.label
  }
  return out
}

function triggerNodeToConfig(node: TriggerNode): Record<string, unknown> {
  const config: Record<string, unknown> = { ...node.rawConfig }
  const value = node.triggerValue.trim()
  if (node.triggerType === 'keyword') {
    const rawKeywords = Array.isArray(config.keywords)
      ? config.keywords.filter((k): k is string => typeof k === 'string')
      : []
    if (value) {
      // Pertahankan daftar kata kunci lama bila kata kunci pertama tidak diubah.
      config.keywords = rawKeywords.length > 0 && rawKeywords[0] === value ? rawKeywords : [value]
    } else {
      delete config.keywords
    }
  } else {
    delete config.keywords
  }
  if (node.triggerType === 'command') {
    if (value) config.command = value
    else delete config.command
  } else {
    delete config.command
  }
  return config
}

/** Petakan node builder menjadi payload simpan workflow. */
export function nodesToWorkflowPayload(
  state: BuilderState,
  meta: { botId: string; name: string; isEnabled: boolean },
): WorkflowPayload {
  const rules = state.conditions.map(conditionNodeToRule)
  return {
    bot_id: meta.botId,
    name: meta.name.trim(),
    trigger_type: state.trigger.triggerType,
    trigger_config: triggerNodeToConfig(state.trigger),
    conditions: state.conditionLogic === 'OR' ? { logic: 'OR', rules } : rules,
    actions: state.actions.map((node) => actionNodeToPayload(node.action)),
    is_enabled: meta.isEnabled,
  }
}

/** Validasi sebelum simpan. Mengembalikan daftar pesan error (kosong = valid). */
export function validateBuilder(state: BuilderState, meta: { botId: string; name: string }): string[] {
  const errors: string[] = []
  if (!meta.botId) errors.push('Pilih bot terlebih dahulu.')
  if (!meta.name.trim()) errors.push('Nama workflow wajib diisi.')
  if (state.trigger.triggerType === 'keyword' && !state.trigger.triggerValue.trim()) {
    errors.push('Trigger kata kunci: kata kunci pemicu tidak boleh kosong.')
  }
  if (state.trigger.triggerType === 'command' && !state.trigger.triggerValue.trim()) {
    errors.push('Trigger command: command pemicu tidak boleh kosong.')
  }
  state.conditions.forEach((condition, i) => {
    if (!condition.value.trim()) {
      errors.push(`Condition ${i + 1}: nilai/kata kunci tidak boleh kosong.`)
    }
  })
  if (state.actions.length === 0) errors.push('Tambahkan minimal satu action.')
  state.actions.forEach((node, i) => {
    const action = node.action
    const label = `Action ${i + 1} (${actionLabel(action.type)})`
    switch (action.type) {
      case 'send_message':
        if (!String(action.text ?? '').trim()) errors.push(`${label}: teks balasan tidak boleh kosong.`)
        break
      case 'notify_admin':
        if (!String(action.text ?? '').trim()) errors.push(`${label}: teks notifikasi tidak boleh kosong.`)
        if (
          action.admin_chat_id !== undefined &&
          action.admin_chat_id !== '' &&
          (!Number.isFinite(Number(action.admin_chat_id)) || Number(action.admin_chat_id) === 0)
        ) {
          errors.push(`${label}: admin_chat_id harus berupa angka.`)
        }
        break
      case 'save_activity':
        if (!String(action.text ?? '').trim()) errors.push(`${label}: teks aktivitas tidak boleh kosong.`)
        break
      case 'send_template':
        if (!String(action.template_id ?? '').trim()) errors.push(`${label}: template_id tidak boleh kosong.`)
        break
      case 'add_label':
        if (!String(action.label ?? '').trim()) errors.push(`${label}: nama label tidak boleh kosong.`)
        break
      case 'save_note':
        if (!String(action.note ?? '').trim()) errors.push(`${label}: isi catatan tidak boleh kosong.`)
        break
      case 'delay': {
        const seconds = Number(action.seconds ?? action.delay_seconds ?? 1)
        if (!Number.isFinite(seconds) || seconds < 0) errors.push(`${label}: durasi delay tidak valid.`)
        break
      }
      default:
        // Backend menolak tipe action di luar ACTION_TYPES saat menyimpan.
        // Data lama tetap termuat di builder, tetapi harus diganti tipenya.
        if (!ACTION_TYPES.some((t) => t.value === action.type)) {
          errors.push(`${label}: tipe action tidak didukung backend, ganti dengan tipe lain.`)
        }
        break
    }
  })
  return errors
}

/** Pindahkan action naik (-1) / turun (+1). Mengembalikan state baru. */
export function moveActionNode(state: BuilderState, index: number, direction: -1 | 1): BuilderState {
  const target = index + direction
  if (index < 0 || index >= state.actions.length || target < 0 || target >= state.actions.length) {
    return state
  }
  const actions = [...state.actions]
  const [moved] = actions.splice(index, 1)
  actions.splice(target, 0, moved)
  return { ...state, actions }
}

export function triggerLabel(triggerType?: string): string {
  return TRIGGER_TYPES.find((t) => t.value === triggerType)?.label ?? triggerType ?? 'Trigger'
}

export function actionLabel(actionType?: string): string {
  return ACTION_TYPES.find((t) => t.value === actionType)?.label ?? actionType ?? 'Action'
}

export function conditionFieldLabel(field?: string): string {
  return CONDITION_FIELDS.find((f) => f.value === field)?.label ?? field ?? 'Isi pesan'
}

export function conditionOperatorLabel(operator?: string): string {
  return CONDITION_OPERATORS.find((o) => o.value === operator)?.label ?? operator ?? 'mengandung'
}

export function triggerSummary(node: TriggerNode): string {
  if (node.triggerType === 'keyword') return node.triggerValue ? `Kata kunci: "${node.triggerValue}"` : 'Kata kunci belum diisi'
  if (node.triggerType === 'command') return node.triggerValue ? `Command: ${node.triggerValue}` : 'Command belum diisi'
  return triggerLabel(node.triggerType)
}

export function conditionSummary(node: ConditionNode): string {
  return `${conditionFieldLabel(node.field)} ${conditionOperatorLabel(node.operator)} "${node.value}"`
}

export function actionSummary(action: WorkflowAction): string {
  switch (action.type) {
    case 'send_message':
    case 'reply':
      return String(action.text ?? '').trim() || 'Teks balasan belum diisi'
    case 'save_activity':
      return String(action.text ?? '').trim() || 'Teks aktivitas belum diisi'
    case 'notify_admin':
      return String(action.text ?? '').trim() || 'Teks notifikasi belum diisi'
    case 'send_template':
      return action.template_id ? `Template: ${action.template_id}` : 'Template belum dipilih'
    case 'add_label':
      return action.label ? `Label: ${action.label}` : 'Label belum diisi'
    case 'save_note':
      return String(action.note ?? '').trim() || 'Catatan belum diisi'
    case 'delay':
      return `Tunda ${Number(action.seconds ?? action.delay_seconds ?? 1)} detik`
    default:
      return String(action.text ?? '').trim() || actionLabel(action.type)
  }
}
