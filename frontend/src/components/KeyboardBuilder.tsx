import { useState } from 'react'
import { Plus, Trash2 } from 'lucide-react'

export interface KeyboardButton {
  text: string
  callback_data?: string
  url?: string
}

export type KeyboardRows = KeyboardButton[][]

export function KeyboardBuilder({
  kind,
  rows,
  onChange,
}: {
  kind: 'inline' | 'reply'
  rows: KeyboardRows
  onChange: (rows: KeyboardRows) => void
}) {
  const [showJson, setShowJson] = useState(false)

  const addRow = () => onChange([...rows, [{ text: '' }]])
  const removeRow = (ri: number) => onChange(rows.filter((_, i) => i !== ri))
  const addButton = (ri: number) => {
    const next = rows.map((r, i) => (i === ri ? [...r, { text: '' }] : r))
    onChange(next)
  }
  const removeButton = (ri: number, bi: number) => {
    const next = rows.map((r, i) => (i === ri ? r.filter((_, j) => j !== bi) : r)).filter((r) => r.length > 0)
    onChange(next)
  }
  const updateButton = (ri: number, bi: number, patch: Partial<KeyboardButton>) => {
    const next = rows.map((r, i) => (i === ri ? r.map((b, j) => (j === bi ? { ...b, ...patch } : b)) : r))
    onChange(next)
  }

  const payload =
    kind === 'inline'
      ? { inline_keyboard: rows.map((r) => r.map((b) => ({ text: b.text, callback_data: b.callback_data ?? b.text, ...(b.url ? { url: b.url } : {}) }))) }
      : { keyboard: rows.map((r) => r.map((b) => ({ text: b.text }))), resize_keyboard: true, one_time_keyboard: false }

  return (
    <div className="space-y-3">
      {rows.length === 0 && (
        <p className="text-sm text-ink/60 dark:text-darktext/60">
          Belum ada tombol. Tambah baris untuk mulai membuat keyboard.
        </p>
      )}
      {rows.map((row, ri) => (
        <div key={ri} className="rounded-xl border border-ink/10 p-3 dark:border-darktext/10">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs font-semibold text-ink/60 dark:text-darktext/60">Baris {ri + 1}</span>
            <button
              type="button"
              onClick={() => removeRow(ri)}
              className="flex items-center gap-1 text-xs font-medium text-danger"
            >
              <Trash2 className="h-3.5 w-3.5" /> Hapus baris
            </button>
          </div>
          <div className="space-y-2">
            {row.map((btn, bi) => (
              <div key={bi} className="flex flex-wrap items-center gap-2">
                <input
                  value={btn.text}
                  onChange={(e) => updateButton(ri, bi, { text: e.target.value })}
                  placeholder="Teks tombol"
                  className="min-w-0 flex-1 rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg"
                />
                {kind === 'inline' && (
                  <>
                    <input
                      value={btn.callback_data ?? ''}
                      onChange={(e) => updateButton(ri, bi, { callback_data: e.target.value })}
                      placeholder="callback_data (opsional)"
                      className="min-w-0 flex-1 rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg"
                    />
                    <input
                      value={btn.url ?? ''}
                      onChange={(e) => updateButton(ri, bi, { url: e.target.value })}
                      placeholder="URL (opsional)"
                      className="min-w-0 flex-1 rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg"
                    />
                  </>
                )}
                <button
                  type="button"
                  onClick={() => removeButton(ri, bi)}
                  aria-label="Hapus tombol"
                  className="rounded-full p-1.5 text-danger hover:bg-danger/10"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            ))}
          </div>
          <button
            type="button"
            onClick={() => addButton(ri)}
            className="mt-2 flex items-center gap-1 text-xs font-semibold text-primary"
          >
            <Plus className="h-3.5 w-3.5" /> Tambah tombol di baris ini
          </button>
        </div>
      ))}
      <div className="flex flex-wrap gap-2">
        <button
          type="button"
          onClick={addRow}
          className="flex items-center gap-1 rounded-full bg-primary/10 px-3 py-1.5 text-sm font-semibold text-primary"
        >
          <Plus className="h-4 w-4" /> Tambah baris
        </button>
        <button
          type="button"
          onClick={() => setShowJson((s) => !s)}
          className="rounded-full bg-ink/10 px-3 py-1.5 text-sm font-semibold dark:bg-darktext/10"
        >
          {showJson ? 'Sembunyikan JSON' : 'Lihat JSON'}
        </button>
      </div>
      {showJson && (
        <pre className="overflow-x-auto rounded-xl bg-ink p-3 text-xs text-white dark:bg-black/60">
          {JSON.stringify(payload, null, 2)}
        </pre>
      )}
    </div>
  )
}

export function buildKeyboardPayload(
  kind: 'inline' | 'reply' | 'none',
  rows: KeyboardRows,
): Record<string, unknown> | undefined {
  if (kind === 'none' || rows.length === 0) return undefined
  if (kind === 'inline') {
    return {
      inline_keyboard: rows.map((r) =>
        // Telegram menolak tombol yang punya callback_data DAN url sekaligus.
        r.map((b) =>
          b.url ? { text: b.text, url: b.url } : { text: b.text, callback_data: b.callback_data || b.text },
        ),
      ),
    }
  }
  return {
    keyboard: rows.map((r) => r.map((b) => ({ text: b.text }))),
    resize_keyboard: true,
    one_time_keyboard: false,
  }
}
