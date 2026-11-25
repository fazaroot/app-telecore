import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Copy, KeyRound, Lock, ShieldCheck, Trash2 } from 'lucide-react'
import { changePin, fetchSecurityStatus, fetchSessions } from '../api/misc'
import { createPersonalToken, fetchPersonalTokens, revokePersonalToken } from '../api/misc'
import { copyText } from '../utils/copy'
import { useSettingsStore, useUiStore } from '../stores/settings'
import { clearPinLocal, setPinLocal, usePinStore } from '../stores/pin'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { ConfirmDialog } from '../components/Modal'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

export function SecurityPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const autoLockMinutes = useSettingsStore((s) => s.autoLockMinutes)
  const setAutoLockMinutes = useSettingsStore((s) => s.setAutoLockMinutes)
  const pinSet = usePinStore((s) => s.pinSet)
  const refreshPinSet = usePinStore((s) => s.refreshPinSet)

  const [currentPin, setCurrentPin] = useState('')
  const [newPin, setNewPin] = useState('')
  const [confirmPin, setConfirmPin] = useState('')
  const [confirmClear, setConfirmClear] = useState(false)
  const [tokenName, setTokenName] = useState('')
  const [createdToken, setCreatedToken] = useState<string | null>(null)
  const [revokingToken, setRevokingToken] = useState<{ id: string | number; name?: string } | null>(null)

  const statusQuery = useQuery({ queryKey: ['security-status'], queryFn: fetchSecurityStatus })
  const sessionsQuery = useQuery({ queryKey: ['sessions'], queryFn: fetchSessions })
  const tokensQuery = useQuery({ queryKey: ['personal-tokens'], queryFn: fetchPersonalTokens })

  const createTokenMutation = useMutation({
    mutationFn: () => createPersonalToken(tokenName.trim()),
    onSuccess: (t) => {
      // Token penuh hanya ditampilkan sekali — backend tidak menyimpannya utuh.
      setCreatedToken(t.token ?? '')
      setTokenName('')
      pushToast('Token dibuat. Simpan sekarang — token tidak ditampilkan lagi.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['personal-tokens'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const revokeTokenMutation = useMutation({
    mutationFn: (id: string | number) => revokePersonalToken(id),
    onSuccess: () => {
      pushToast('Token dicabut (revoke).', 'success')
      setRevokingToken(null)
      void queryClient.invalidateQueries({ queryKey: ['personal-tokens'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const tokens = tokensQuery.data ?? []

  const changePinMutation = useMutation({
    mutationFn: async () => {
      // PIN dikirim ke backend untuk di-hash di server; frontend tidak pernah
      // meng-hash PIN untuk keperluan server. PIN lokal (app lock) disimpan
      // sebagai hash SHA-256 + salt via Preferences.
      await changePin({ current_pin: currentPin || undefined, new_pin: newPin })
      await setPinLocal(newPin)
    },
    onSuccess: async () => {
      pushToast('PIN berhasil diubah dan app lock diaktifkan.', 'success')
      setCurrentPin('')
      setNewPin('')
      setConfirmPin('')
      await refreshPinSet()
      void queryClient.invalidateQueries({ queryKey: ['security-status'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const clearPinMutation = useMutation({
    mutationFn: clearPinLocal,
    onSuccess: async () => {
      pushToast('App lock dimatikan.', 'success')
      setConfirmClear(false)
      await refreshPinSet()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const pinValid = /^\d{4,6}$/.test(newPin) && newPin === confirmPin
  const status = statusQuery.data
  const sessions = sessionsQuery.data ?? status?.sessions ?? []

  return (
    <div className="space-y-4">
      <PageHeader title="Security" subtitle="Keamanan token, PIN, dan sesi" />

      <Card>
        <h2 className="mb-2 flex items-center gap-2 font-bold">
          <ShieldCheck className="h-5 w-5 text-success" /> Status Enkripsi
        </h2>
        {statusQuery.isLoading ? (
          <LoadingSpinner label="Memuat status keamanan…" />
        ) : statusQuery.isError ? (
          <ErrorState message={(statusQuery.error as Error).message} onRetry={() => void statusQuery.refetch()} />
        ) : (
          <dl className="space-y-2 text-sm">
            <div className="flex justify-between">
              <dt className="text-ink/60 dark:text-darktext/60">Enkripsi token</dt>
              <dd>
                <Badge tone={status?.encryption_enabled === false ? 'danger' : 'success'}>
                  {status?.token_encryption ?? (status?.encryption_enabled === false ? 'Nonaktif' : 'Aktif (AES-256-GCM)')}
                </Badge>
              </dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-ink/60 dark:text-darktext/60">Masking token</dt>
              <dd className="font-medium">Token hanya ditampilkan tersamar (****abcd)</dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-ink/60 dark:text-darktext/60">PIN server</dt>
              <dd className="font-medium">{status?.pin_set ? 'Sudah diatur' : 'Belum diatur'}</dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-ink/60 dark:text-darktext/60">App lock lokal</dt>
              <dd className="font-medium">{pinSet ? 'Aktif' : 'Nonaktif'}</dd>
            </div>
          </dl>
        )}
      </Card>

      <Card>
        <h2 className="mb-2 flex items-center gap-2 font-bold">
          <KeyRound className="h-5 w-5" /> PIN App Lock
        </h2>
        <p className="mb-3 rounded-xl bg-primary/10 p-3 text-sm text-ink/80 dark:text-darktext/80">
          PIN 4–6 digit mengunci aplikasi saat dibuka. PIN dikirim ke backend untuk di-hash di server;
          di perangkat ini hanya hash SHA-256 + salt yang disimpan — PIN mentah tidak pernah disimpan.
          Ini kunci level aplikasi, <strong>bukan</strong> hardware-backed.
          Biometrik (sidik jari/face unlock) <strong>tidak tersedia di v1</strong>.
        </p>
        <div className="space-y-3">
          {pinSet && (
            <label className="block text-sm">
              <span className="mb-1 block font-medium">PIN saat ini</span>
              <input
                type="password"
                inputMode="numeric"
                value={currentPin}
                onChange={(e) => setCurrentPin(e.target.value.replace(/\D/g, '').slice(0, 6))}
                placeholder="PIN lama"
                className={inputCls}
              />
            </label>
          )}
          <label className="block text-sm">
            <span className="mb-1 block font-medium">PIN baru (4–6 digit)</span>
            <input
              type="password"
              inputMode="numeric"
              value={newPin}
              onChange={(e) => setNewPin(e.target.value.replace(/\D/g, '').slice(0, 6))}
              placeholder="••••••"
              className={inputCls}
            />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Konfirmasi PIN baru</span>
            <input
              type="password"
              inputMode="numeric"
              value={confirmPin}
              onChange={(e) => setConfirmPin(e.target.value.replace(/\D/g, '').slice(0, 6))}
              placeholder="••••••"
              className={inputCls}
            />
          </label>
          {newPin && confirmPin && newPin !== confirmPin && (
            <p className="text-sm text-danger">PIN baru dan konfirmasi tidak sama.</p>
          )}
          <button
            type="button"
            disabled={!pinValid || changePinMutation.isPending}
            onClick={() => changePinMutation.mutate()}
            className="w-full rounded-full bg-primary py-2.5 text-sm font-bold text-white disabled:opacity-50"
          >
            {changePinMutation.isPending ? 'Menyimpan…' : pinSet ? 'Ubah PIN' : 'Atur PIN'}
          </button>
          {pinSet && (
            <button
              type="button"
              onClick={() => setConfirmClear(true)}
              disabled={clearPinMutation.isPending}
              className="w-full rounded-full bg-danger/10 py-2.5 text-sm font-bold text-danger disabled:opacity-50"
            >
              Matikan App Lock
            </button>
          )}
        </div>
      </Card>

      <Card>
        <h2 className="mb-2 flex items-center gap-2 font-bold">
          <KeyRound className="h-5 w-5" /> API Token Pribadi
        </h2>
        <p className="mb-3 text-sm text-ink/70 dark:text-darktext/70">
          Token pribadi dipakai untuk memanggil API TeleCore (termasuk MCP) tanpa memakai API key
          utama. Token hanya ditampilkan penuh sekali saat dibuat — simpan di tempat aman.
        </p>
        <div className="mb-3 flex gap-2">
          <input
            value={tokenName}
            onChange={(e) => setTokenName(e.target.value)}
            placeholder="Nama token, mis. Claude Desktop"
            className={inputCls}
          />
          <button
            type="button"
            disabled={!tokenName.trim() || createTokenMutation.isPending}
            onClick={() => createTokenMutation.mutate()}
            className="shrink-0 rounded-full bg-primary px-4 py-2 text-sm font-bold text-white disabled:opacity-50"
          >
            {createTokenMutation.isPending ? 'Membuat…' : 'Buat Token'}
          </button>
        </div>
        {createdToken && (
          <div className="mb-3 rounded-xl border border-amber-500/40 p-3">
            <p className="mb-2 text-sm font-semibold text-amber-700 dark:text-amber-400">
              Token baru — tampil sekali saja, salin dan simpan sekarang:
            </p>
            <div className="flex gap-2">
              <input value={createdToken} readOnly className={`${inputCls} font-mono`} />
              <button
                type="button"
                onClick={async () => {
                  const ok = await copyText(createdToken)
                  pushToast(ok ? 'Token disalin.' : 'Gagal menyalin.', ok ? 'success' : 'error')
                }}
                className="flex shrink-0 items-center gap-1 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
              >
                <Copy className="h-3.5 w-3.5" /> Salin
              </button>
            </div>
            <button
              type="button"
              onClick={() => setCreatedToken(null)}
              className="mt-2 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
            >
              Saya sudah menyimpan token — tutup
            </button>
          </div>
        )}
        {tokensQuery.isLoading ? (
          <LoadingSpinner label="Memuat token…" />
        ) : tokensQuery.isError ? (
          <ErrorState message={(tokensQuery.error as Error).message} onRetry={() => void tokensQuery.refetch()} />
        ) : tokens.length === 0 ? (
          <EmptyState title="Belum ada token pribadi" description="Buat token untuk integrasi eksternal seperti MCP." />
        ) : (
          <ul className="space-y-2">
            {tokens.map((t) => (
              <li key={String(t.id)} className="flex items-center justify-between gap-3 rounded-xl border border-ink/10 p-3 text-sm dark:border-darktext/10">
                <div className="min-w-0">
                  <p className="font-semibold">{t.name ?? 'Token'}</p>
                  <p className="font-mono text-xs text-ink/60 dark:text-darktext/60">
                    {t.prefix ?? '—'}… · dibuat{' '}
                    {t.created_at ? new Date(t.created_at).toLocaleDateString('id-ID') : '—'}
                    {t.last_used_at
                      ? ` · terakhir dipakai ${new Date(t.last_used_at).toLocaleString('id-ID')}`
                      : ' · belum pernah dipakai'}
                  </p>
                </div>
                <button
                  type="button"
                  aria-label="Cabut token"
                  onClick={() => setRevokingToken({ id: t.id, name: t.name })}
                  className="flex shrink-0 items-center gap-1 rounded-full bg-danger/10 px-3 py-1.5 text-xs font-semibold text-danger"
                >
                  <Trash2 className="h-3.5 w-3.5" /> Revoke
                </button>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <Card>
        <h2 className="mb-2 flex items-center gap-2 font-bold">
          <Lock className="h-5 w-5" /> Auto-Lock
        </h2>
        <label className="flex items-center justify-between gap-3 text-sm font-medium">
          Kunci otomatis setelah tidak aktif
          <select
            value={autoLockMinutes}
            onChange={(e) => setAutoLockMinutes(Number(e.target.value))}
            className="rounded-lg border border-ink/15 bg-bg px-2 py-1.5 text-sm dark:border-darktext/15 dark:bg-darkbg"
          >
            <option value={0}>Tidak pernah</option>
            <option value={1}>1 menit</option>
            <option value={5}>5 menit</option>
            <option value={15}>15 menit</option>
            <option value={30}>30 menit</option>
          </select>
        </label>
        {!pinSet && (
          <p className="mt-2 text-sm text-ink/50 dark:text-darktext/50">
            Auto-lock hanya berlaku bila PIN sudah diatur.
          </p>
        )}
      </Card>

      <Card>
        <h2 className="mb-2 font-bold">Sesi Aktif</h2>
        {sessionsQuery.isLoading ? (
          <LoadingSpinner label="Memuat sesi…" />
        ) : sessionsQuery.isError ? (
          <ErrorState message={(sessionsQuery.error as Error).message} onRetry={() => void sessionsQuery.refetch()} />
        ) : sessions.length === 0 ? (
          <EmptyState title="Tidak ada data sesi" description="Backend belum mengembalikan daftar sesi." />
        ) : (
          <ul className="space-y-2">
            {sessions.map((s) => (
              <li key={String(s.id)} className="rounded-xl border border-ink/10 p-3 text-sm dark:border-darktext/10">
                <p className="font-semibold">{s.device ?? 'Perangkat tidak dikenal'}</p>
                <p className="text-ink/60 dark:text-darktext/60">
                  {s.ip ?? '—'} · Terakhir aktif:{' '}
                  {s.last_active ? new Date(s.last_active).toLocaleString('id-ID') : '—'}
                </p>
              </li>
            ))}
          </ul>
        )}
      </Card>

      <ConfirmDialog
        open={revokingToken !== null}
        onClose={() => setRevokingToken(null)}
        onConfirm={() => revokingToken && revokeTokenMutation.mutate(revokingToken.id)}
        title="Cabut Token"
        message={`Cabut token "${revokingToken?.name ?? ''}"? Integrasi yang memakai token ini akan langsung berhenti berfungsi.`}
        busy={revokeTokenMutation.isPending}
      />

      <ConfirmDialog
        open={confirmClear}
        onClose={() => setConfirmClear(false)}
        onConfirm={() => clearPinMutation.mutate()}
        title="Matikan App Lock"
        message="PIN lokal akan dihapus dari perangkat ini dan aplikasi tidak lagi terkunci saat dibuka."
        confirmLabel="Matikan"
        busy={clearPinMutation.isPending}
      />
    </div>
  )
}
