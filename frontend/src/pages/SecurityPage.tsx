import { useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { KeyRound, Lock, ShieldCheck } from 'lucide-react'
import { changePin, fetchSecurityStatus, fetchSessions } from '../api/misc'
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

  const statusQuery = useQuery({ queryKey: ['security-status'], queryFn: fetchSecurityStatus })
  const sessionsQuery = useQuery({ queryKey: ['sessions'], queryFn: fetchSessions })

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
