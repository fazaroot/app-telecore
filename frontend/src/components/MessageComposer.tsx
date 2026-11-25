import { useMemo, useRef, useState } from 'react'
import { Paperclip, Send, X } from 'lucide-react'
import type { Bot, LocalTemplate } from '../types'
import { previewMessage } from '../api/messages'
import { KeyboardBuilder, buildKeyboardPayload, type KeyboardRows } from './KeyboardBuilder'

export interface ComposerValue {
  botId: string
  chatId: string
  text: string
  parseMode: 'MarkdownV2' | 'HTML' | 'none'
  messageType: string
  keyboard?: Record<string, unknown>
  media?: { type: string; name: string; data_url: string }
}

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

const MEDIA_TYPES = [
  { value: 'text', label: 'Teks' },
  { value: 'photo', label: 'Foto' },
  { value: 'document', label: 'Dokumen' },
  { value: 'video', label: 'Video' },
  { value: 'audio', label: 'Audio' },
  { value: 'animation', label: 'Animasi (GIF)' },
]

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result ?? ''))
    reader.onerror = () => reject(new Error('Gagal membaca file'))
    reader.readAsDataURL(file)
  })
}

export function MessageComposer({
  bots,
  templates,
  onSend,
  sending,
  onSaveTemplate,
}: {
  bots: Bot[]
  templates: LocalTemplate[]
  onSend: (value: ComposerValue) => void
  sending: boolean
  onSaveTemplate: (name: string, text: string, parseMode: 'MarkdownV2' | 'HTML' | 'none') => void
}) {
  const [botId, setBotId] = useState('')
  const [chatId, setChatId] = useState('')
  const [text, setText] = useState('')
  const [parseMode, setParseMode] = useState<'MarkdownV2' | 'HTML' | 'none'>('none')
  const [messageType, setMessageType] = useState('text')
  const [keyboardKind, setKeyboardKind] = useState<'none' | 'inline' | 'reply'>('none')
  const [keyboardRows, setKeyboardRows] = useState<KeyboardRows>([])
  const [mediaFile, setMediaFile] = useState<File | null>(null)
  const [showPreview, setShowPreview] = useState(true)
  const [templateName, setTemplateName] = useState('')
  // Preview variabel template ({{first_name}}, {{username}}, {{chat_title}})
  const [previewFirstName, setPreviewFirstName] = useState('')
  const [previewUsername, setPreviewUsername] = useState('')
  const [previewChatTitle, setPreviewChatTitle] = useState('')
  const [renderedPreview, setRenderedPreview] = useState<string | null>(null)
  const [previewBusy, setPreviewBusy] = useState(false)
  const [previewError, setPreviewError] = useState('')
  const fileRef = useRef<HTMLInputElement>(null)

  const chatIdValid = /^-?\d+$/.test(chatId.trim())

  const canSend = useMemo(
    () => Boolean(botId && chatIdValid && (text.trim() || mediaFile)),
    [botId, chatIdValid, text, mediaFile],
  )

  const handleSend = async () => {
    if (!canSend) return
    let media: ComposerValue['media']
    if (mediaFile) {
      try {
        const data_url = await readFileAsDataUrl(mediaFile)
        media = { type: messageType, name: mediaFile.name, data_url }
      } catch {
        return
      }
    }
    onSend({
      botId,
      chatId: chatId.trim(),
      text: text.trim(),
      parseMode,
      messageType: mediaFile ? messageType : 'text',
      keyboard: buildKeyboardPayload(keyboardKind, keyboardRows),
      media,
    })
  }

  const resetAfterSend = () => {
    setText('')
    setMediaFile(null)
    setKeyboardRows([])
    setKeyboardKind('none')
  }

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <label className="block text-sm">
          <span className="mb-1 block font-medium text-ink/70 dark:text-darktext/70">Bot pengirim</span>
          <select value={botId} onChange={(e) => setBotId(e.target.value)} className={inputCls}>
            <option value="">Pilih bot…</option>
            {bots.map((b) => (
              <option key={String(b.id)} value={String(b.id)}>
                {b.name ?? b.username ?? `Bot ${b.id}`}
                {b.username ? ` (@${b.username})` : ''}
              </option>
            ))}
          </select>
        </label>
        <label className="block text-sm">
          <span className="mb-1 block font-medium text-ink/70 dark:text-darktext/70">Chat ID tujuan</span>
          <input
            value={chatId}
            onChange={(e) => setChatId(e.target.value)}
            placeholder="mis. 123456789 (ID numerik — lihat di halaman Users)"
            inputMode="text"
            className={inputCls}
          />
          {chatId.trim() && !chatIdValid && (
            <span className="mt-1 block text-xs text-danger">
              Chat ID harus angka (ID grup diawali minus, mis. -1001234567890). Username @… belum
              didukung di v1.
            </span>
          )}
        </label>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <select
          value={messageType}
          onChange={(e) => setMessageType(e.target.value)}
          className={`${inputCls} !w-auto`}
          aria-label="Tipe pesan"
        >
          {MEDIA_TYPES.map((t) => (
            <option key={t.value} value={t.value}>
              {t.label}
            </option>
          ))}
        </select>
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
          onClick={() => fileRef.current?.click()}
          className="flex items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-sm font-semibold dark:bg-darktext/10"
        >
          <Paperclip className="h-4 w-4" /> Lampirkan media
        </button>
        <input
          ref={fileRef}
          type="file"
          className="hidden"
          onChange={(e) => setMediaFile(e.target.files?.[0] ?? null)}
        />
      </div>

      {mediaFile && (
        <div className="flex items-center justify-between rounded-xl bg-primary/10 px-3 py-2 text-sm">
          <span className="truncate">
            {mediaFile.name} ({Math.round(mediaFile.size / 1024)} KB)
          </span>
          <button type="button" aria-label="Hapus lampiran" onClick={() => setMediaFile(null)} className="p-1 text-danger">
            <X className="h-4 w-4" />
          </button>
        </div>
      )}

      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Tulis pesan…"
        rows={4}
        className={inputCls}
      />

      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => setShowPreview((s) => !s)}
          className="rounded-full bg-ink/10 px-3 py-1.5 text-sm font-semibold dark:bg-darktext/10"
        >
          {showPreview ? 'Sembunyikan preview' : 'Tampilkan preview'}
        </button>
        {templates.length > 0 && (
          <select
            value=""
            onChange={(e) => {
              const t = templates.find((x) => x.id === e.target.value)
              if (t) {
                setText(t.text)
                setParseMode(t.parse_mode)
              }
            }}
            className={`${inputCls} !w-auto`}
            aria-label="Gunakan template lokal"
          >
            <option value="">Gunakan template lokal…</option>
            {templates.map((t) => (
              <option key={t.id} value={t.id}>
                {t.name}
              </option>
            ))}
          </select>
        )}
      </div>

      {showPreview && text && (
        <div className="rounded-xl border border-ink/10 bg-bg p-3 dark:border-darktext/10 dark:bg-darkbg">
          <p className="mb-1 text-xs font-semibold text-ink/50 dark:text-darktext/50">
            Preview ({parseMode === 'none' ? 'teks biasa' : parseMode})
          </p>
          <p className="whitespace-pre-wrap text-sm">{text}</p>
        </div>
      )}

      {/* Preview variabel template via backend ({{first_name}}, {{username}}, {{chat_title}}). */}
      <div className="rounded-xl border border-ink/10 p-3 dark:border-darktext/10">
        <p className="mb-2 text-sm font-semibold">Preview variabel template</p>
        <p className="mb-2 text-xs text-ink/50 dark:text-darktext/50">
          Variabel yang didukung: {'{{first_name}}'}, {'{{username}}'}, {'{{chat_title}}'}. Isi contoh
          nilai di bawah lalu tekan Preview — hasil render berasal dari server.
        </p>
        <div className="mb-2 grid grid-cols-1 gap-2 sm:grid-cols-3">
          <input
            value={previewFirstName}
            onChange={(e) => setPreviewFirstName(e.target.value)}
            placeholder="Nama depan (first_name)"
            className={inputCls}
          />
          <input
            value={previewUsername}
            onChange={(e) => setPreviewUsername(e.target.value)}
            placeholder="Username"
            className={inputCls}
          />
          <input
            value={previewChatTitle}
            onChange={(e) => setPreviewChatTitle(e.target.value)}
            placeholder="Judul chat (chat_title)"
            className={inputCls}
          />
        </div>
        <button
          type="button"
          disabled={!text.trim() || previewBusy}
          onClick={async () => {
            setPreviewBusy(true)
            setPreviewError('')
            try {
              const rendered = await previewMessage({
                text: text.trim(),
                first_name: previewFirstName.trim() || undefined,
                username: previewUsername.trim() || undefined,
                chat_title: previewChatTitle.trim() || undefined,
              })
              setRenderedPreview(rendered)
            } catch (e) {
              setPreviewError((e as Error).message)
              setRenderedPreview(null)
            } finally {
              setPreviewBusy(false)
            }
          }}
          className="rounded-full bg-primary/10 px-3 py-1.5 text-sm font-semibold text-primary disabled:opacity-50"
        >
          {previewBusy ? 'Memproses…' : 'Preview'}
        </button>
        {previewError && <p className="mt-2 text-sm text-danger">{previewError}</p>}
        {renderedPreview !== null && (
          <div className="mt-2 rounded-xl bg-bg p-3 dark:bg-darkbg">
            <p className="mb-1 text-xs font-semibold text-ink/50 dark:text-darktext/50">Hasil render server</p>
            <p className="whitespace-pre-wrap text-sm">{renderedPreview}</p>
          </div>
        )}
      </div>

      <div>
        <p className="mb-2 text-sm font-semibold text-ink dark:text-darktext">Keyboard (opsional)</p>
        <div className="mb-3 flex gap-2">
          {(['none', 'inline', 'reply'] as const).map((k) => (
            <button
              key={k}
              type="button"
              onClick={() => setKeyboardKind(k)}
              className={`rounded-full px-3 py-1.5 text-sm font-semibold ${
                keyboardKind === k ? 'bg-primary text-white' : 'bg-ink/10 dark:bg-darktext/10'
              }`}
            >
              {k === 'none' ? 'Tanpa keyboard' : k === 'inline' ? 'Inline keyboard' : 'Reply keyboard'}
            </button>
          ))}
        </div>
        {keyboardKind !== 'none' && (
          <KeyboardBuilder kind={keyboardKind} rows={keyboardRows} onChange={setKeyboardRows} />
        )}
      </div>

      <div className="flex flex-wrap items-center gap-2 rounded-xl border border-ink/10 p-3 dark:border-darktext/10">
        <input
          value={templateName}
          onChange={(e) => setTemplateName(e.target.value)}
          placeholder="Nama template lokal"
          className={`${inputCls} !w-auto flex-1`}
        />
        <button
          type="button"
          disabled={!templateName.trim() || !text.trim()}
          onClick={() => {
            onSaveTemplate(templateName.trim(), text.trim(), parseMode)
            setTemplateName('')
          }}
          className="rounded-full bg-secondary/20 px-3 py-1.5 text-sm font-semibold text-primary disabled:opacity-50"
        >
          Simpan sebagai template lokal
        </button>
        <span className="w-full text-xs text-ink/50 dark:text-darktext/50">
          Template disimpan di perangkat ini saja (template lokal perangkat), tidak tersimpan di server.
        </span>
      </div>

      <button
        type="button"
        onClick={async () => {
          await handleSend()
          resetAfterSend()
        }}
        disabled={!canSend || sending}
        className="flex w-full items-center justify-center gap-2 rounded-full bg-primary py-3 text-sm font-bold text-white disabled:opacity-50"
      >
        <Send className="h-4 w-4" />
        {sending ? 'Mengirim…' : 'Kirim pesan'}
      </button>
    </div>
  )
}
