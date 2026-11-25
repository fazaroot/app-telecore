import { Plus, Trash2 } from 'lucide-react'
import type { WorkflowAction, WorkflowCondition } from '../types'

export const TRIGGER_TYPES = [
  { value: 'message_received', label: 'Pesan diterima' },
  { value: 'command', label: 'Command diterima' },
  { value: 'new_user', label: 'Pengguna baru' },
  { value: 'keyword', label: 'Kata kunci cocok' },
  { value: 'schedule', label: 'Jadwal' },
]

export const ACTION_TYPES = [
  { value: 'send_message', label: 'Kirim pesan' },
  { value: 'reply', label: 'Balas pesan' },
  { value: 'forward', label: 'Teruskan pesan' },
  { value: 'add_label', label: 'Tambah label pengguna' },
  { value: 'block_user', label: 'Blokir pengguna' },
  { value: 'notify_admin', label: 'Beri tahu admin' },
]

export const CONDITION_FIELDS = [
  { value: 'text', label: 'Isi pesan' },
  { value: 'chat_id', label: 'Chat ID' },
  { value: 'user_id', label: 'User ID' },
  { value: 'message_type', label: 'Tipe pesan' },
]

export const CONDITION_OPERATORS = [
  { value: 'contains', label: 'mengandung' },
  { value: 'equals', label: 'sama dengan' },
  { value: 'starts_with', label: 'diawali' },
  { value: 'ends_with', label: 'diakhiri' },
]

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

/**
 * Builder workflow berbasis langkah (form). Bukan drag-drop — setiap langkah
 * ditambah/diubah lewat form, dan itu sudah cukup untuk workflow yang berfungsi.
 */
export function WorkflowBuilder({
  triggerType,
  triggerValue,
  conditions,
  actions,
  onTriggerTypeChange,
  onTriggerValueChange,
  onConditionsChange,
  onActionsChange,
}: {
  triggerType: string
  triggerValue: string
  conditions: WorkflowCondition[]
  actions: WorkflowAction[]
  onTriggerTypeChange: (v: string) => void
  onTriggerValueChange: (v: string) => void
  onConditionsChange: (c: WorkflowCondition[]) => void
  onActionsChange: (a: WorkflowAction[]) => void
}) {
  return (
    <div className="space-y-5">
      {/* Langkah 1: Trigger */}
      <section>
        <h3 className="mb-2 text-sm font-bold text-ink dark:text-darktext">Langkah 1 — Trigger</h3>
        <div className="space-y-2">
          <select value={triggerType} onChange={(e) => onTriggerTypeChange(e.target.value)} className={inputCls}>
            {TRIGGER_TYPES.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </select>
          {(triggerType === 'keyword' || triggerType === 'command') && (
            <input
              value={triggerValue}
              onChange={(e) => onTriggerValueChange(e.target.value)}
              placeholder={triggerType === 'keyword' ? 'Kata kunci pemicu, mis. "harga"' : 'Command, mis. /start'}
              className={inputCls}
            />
          )}
        </div>
      </section>

      {/* Langkah 2: Conditions */}
      <section>
        <h3 className="mb-2 text-sm font-bold text-ink dark:text-darktext">
          Langkah 2 — Kondisi <span className="font-normal text-ink/50 dark:text-darktext/50">(opsional)</span>
        </h3>
        {conditions.length === 0 && (
          <p className="mb-2 text-sm text-ink/60 dark:text-darktext/60">
            Tanpa kondisi, workflow berjalan untuk semua pemicu.
          </p>
        )}
        <div className="space-y-2">
          {conditions.map((c, i) => (
            <div key={i} className="flex flex-wrap items-center gap-2">
              <select
                value={c.field ?? 'text'}
                onChange={(e) =>
                  onConditionsChange(conditions.map((x, j) => (j === i ? { ...x, field: e.target.value } : x)))
                }
                className={`${inputCls} !w-auto flex-1`}
              >
                {CONDITION_FIELDS.map((f) => (
                  <option key={f.value} value={f.value}>
                    {f.label}
                  </option>
                ))}
              </select>
              <select
                value={c.operator ?? 'contains'}
                onChange={(e) =>
                  onConditionsChange(conditions.map((x, j) => (j === i ? { ...x, operator: e.target.value } : x)))
                }
                className={`${inputCls} !w-auto flex-1`}
              >
                {CONDITION_OPERATORS.map((o) => (
                  <option key={o.value} value={o.value}>
                    {o.label}
                  </option>
                ))}
              </select>
              <input
                value={c.value ?? ''}
                onChange={(e) =>
                  onConditionsChange(conditions.map((x, j) => (j === i ? { ...x, value: e.target.value } : x)))
                }
                placeholder="Nilai"
                className={`${inputCls} !w-auto flex-1`}
              />
              <button
                type="button"
                aria-label="Hapus kondisi"
                onClick={() => onConditionsChange(conditions.filter((_, j) => j !== i))}
                className="rounded-full p-1.5 text-danger hover:bg-danger/10"
              >
                <Trash2 className="h-4 w-4" />
              </button>
            </div>
          ))}
        </div>
        <button
          type="button"
          onClick={() => onConditionsChange([...conditions, { field: 'text', operator: 'contains', value: '' }])}
          className="mt-2 flex items-center gap-1 text-sm font-semibold text-primary"
        >
          <Plus className="h-4 w-4" /> Tambah kondisi
        </button>
      </section>

      {/* Langkah 3: Actions */}
      <section>
        <h3 className="mb-2 text-sm font-bold text-ink dark:text-darktext">Langkah 3 — Aksi</h3>
        {actions.length === 0 && (
          <p className="mb-2 text-sm text-ink/60 dark:text-darktext/60">
            Workflow membutuhkan minimal satu aksi.
          </p>
        )}
        <div className="space-y-3">
          {actions.map((a, i) => (
            <div key={i} className="space-y-2 rounded-xl border border-ink/10 p-3 dark:border-darktext/10">
              <div className="flex items-center justify-between">
                <span className="text-xs font-semibold text-ink/60 dark:text-darktext/60">Aksi {i + 1}</span>
                <button
                  type="button"
                  aria-label="Hapus aksi"
                  onClick={() => onActionsChange(actions.filter((_, j) => j !== i))}
                  className="rounded-full p-1.5 text-danger hover:bg-danger/10"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
              <select
                value={a.type ?? 'send_message'}
                onChange={(e) =>
                  onActionsChange(actions.map((x, j) => (j === i ? { ...x, type: e.target.value } : x)))
                }
                className={inputCls}
              >
                {ACTION_TYPES.map((t) => (
                  <option key={t.value} value={t.value}>
                    {t.label}
                  </option>
                ))}
              </select>
              {(a.type === 'send_message' || a.type === 'reply' || a.type === 'notify_admin') && (
                <textarea
                  value={a.text ?? ''}
                  onChange={(e) =>
                    onActionsChange(actions.map((x, j) => (j === i ? { ...x, text: e.target.value } : x)))
                  }
                  placeholder="Isi pesan yang dikirim"
                  rows={2}
                  className={inputCls}
                />
              )}
              {a.type === 'add_label' && (
                <input
                  value={String(a.label ?? '')}
                  onChange={(e) =>
                    onActionsChange(actions.map((x, j) => (j === i ? { ...x, label: e.target.value } : x)))
                  }
                  placeholder="Nama label"
                  className={inputCls}
                />
              )}
              <label className="flex items-center gap-2 text-sm text-ink/70 dark:text-darktext/70">
                Delay sebelum aksi ini (detik)
                <input
                  type="number"
                  min={0}
                  value={a.delay_seconds ?? 0}
                  onChange={(e) =>
                    onActionsChange(
                      actions.map((x, j) =>
                        j === i ? { ...x, delay_seconds: Math.max(0, Number(e.target.value) || 0) } : x,
                      ),
                    )
                  }
                  className="w-20 rounded-lg border border-ink/15 bg-bg px-2 py-1.5 text-sm dark:border-darktext/15 dark:bg-darkbg"
                />
              </label>
            </div>
          ))}
        </div>
        <button
          type="button"
          onClick={() => onActionsChange([...actions, { type: 'send_message', text: '', delay_seconds: 0 }])}
          className="mt-2 flex items-center gap-1 text-sm font-semibold text-primary"
        >
          <Plus className="h-4 w-4" /> Tambah aksi
        </button>
      </section>
    </div>
  )
}
