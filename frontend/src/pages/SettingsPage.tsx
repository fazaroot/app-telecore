import { useEffect, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CheckCircle2, Database, Download, HardDriveDownload, HeartPulse, RefreshCw, Trash2, XCircle } from 'lucide-react'
import { fetchHealth } from '../api/bots'
import { fetchWatchdogStatus, updateWatchdogSettings } from '../api/watchdog'
import { createBackup, deleteBackup, fetchAppSettings, fetchAutoBackupSettings, fetchBackups, restoreBackup, updateAppSettings, updateAutoBackupSettings } from '../api/misc'
import { DEFAULT_BASE_URL, useSettingsStore, useUiStore } from '../stores/settings'
import { Badge, Card, EmptyState, ErrorState, LoadingSpinner, PageHeader } from '../components/ui'
import { ConfirmDialog } from '../components/Modal'
import { AiProvidersSection } from '../components/AiProvidersSection'
import type { AutoBackupSettings, Backup } from '../types'

const inputCls =
  'w-full rounded-lg border border-ink/15 bg-bg px-3 py-2 text-sm dark:border-darktext/15 dark:bg-darkbg'

export function SettingsPage() {
  const queryClient = useQueryClient()
  const pushToast = useUiStore((s) => s.pushToast)
  const baseUrl = useSettingsStore((s) => s.baseUrl)
  const apiKey = useSettingsStore((s) => s.apiKey)
  const theme = useSettingsStore((s) => s.theme)
  const setBaseUrl = useSettingsStore((s) => s.setBaseUrl)
  const setApiKey = useSettingsStore((s) => s.setApiKey)
  const setTheme = useSettingsStore((s) => s.setTheme)

  const [urlDraft, setUrlDraft] = useState(baseUrl)
  const [keyDraft, setKeyDraft] = useState(apiKey)
  const [showKey, setShowKey] = useState(false)
  const [restoring, setRestoring] = useState<Backup | null>(null)
  const [deletingBackup, setDeletingBackup] = useState<Backup | null>(null)
  const [backupPassphrase, setBackupPassphrase] = useState('')
  const passphraseOk = backupPassphrase.length >= 8

  // Watchdog (v1.2)
  const [wdInterval, setWdInterval] = useState('5')
  const [wdAutoRestart, setWdAutoRestart] = useState(true)
  const [wdLoaded, setWdLoaded] = useState(false)
  // Auto-backup (v1.2)
  const [abEnabled, setAbEnabled] = useState(false)
  const [abFrequency, setAbFrequency] = useState<'daily' | 'weekly'>('daily')
  const [abHour, setAbHour] = useState('2')
  const [abRetention, setAbRetention] = useState('7')
  const [abLoaded, setAbLoaded] = useState(false)

  const serverSettingsQuery = useQuery({ queryKey: ['app-settings'], queryFn: fetchAppSettings })
  const backupsQuery = useQuery({ queryKey: ['backups'], queryFn: fetchBackups })
  const watchdogQuery = useQuery({ queryKey: ['watchdog', 'status'], queryFn: fetchWatchdogStatus })
  const autoBackupQuery = useQuery({ queryKey: ['auto-backup-settings'], queryFn: fetchAutoBackupSettings })

  useEffect(() => {
    if (watchdogQuery.data && !wdLoaded) {
      setWdInterval(String(watchdogQuery.data.interval_minutes ?? 5))
      setWdAutoRestart(Boolean(watchdogQuery.data.auto_restart))
      setWdLoaded(true)
    }
  }, [watchdogQuery.data, wdLoaded])

  useEffect(() => {
    if (autoBackupQuery.data && !abLoaded) {
      const s = autoBackupQuery.data
      setAbEnabled(Boolean(s.enabled))
      setAbFrequency((s.frequency as 'daily' | 'weekly') ?? 'daily')
      setAbHour(String(s.hour ?? 2))
      setAbRetention(String(s.retention ?? 7))
      setAbLoaded(true)
    }
  }, [autoBackupQuery.data, abLoaded])

  const testMutation = useMutation({
    mutationFn: async () => {
      // Simpan dulu agar tes memakai nilai terbaru.
      setBaseUrl(urlDraft.trim() || DEFAULT_BASE_URL)
      setApiKey(keyDraft.trim())
      return fetchHealth()
    },
    onSuccess: (health) => {
      pushToast(`Koneksi berhasil — server ${health.status ?? 'ok'} (v${health.version ?? '—'}).`, 'success')
      void queryClient.invalidateQueries({ queryKey: ['health'] })
    },
    onError: (e: Error) => pushToast(`Tes koneksi gagal: ${e.message}`, 'error'),
  })

  const serverThemeMutation = useMutation({
    mutationFn: (t: 'light' | 'dark') => updateAppSettings({ theme: t }),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ['app-settings'] }),
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const handleTheme = (t: 'light' | 'dark') => {
    setTheme(t)
    serverThemeMutation.mutate(t)
  }

  const createBackupMutation = useMutation({
    mutationFn: () => createBackup(backupPassphrase),
    onSuccess: () => {
      pushToast('Backup dibuat.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['backups'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const restoreMutation = useMutation({
    mutationFn: (id: string | number) => restoreBackup(id, backupPassphrase),
    onSuccess: () => {
      pushToast('Backup dipulihkan.', 'success')
      setRestoring(null)
      void queryClient.invalidateQueries()
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const deleteBackupMutation = useMutation({
    mutationFn: (id: string | number) => deleteBackup(id),
    onSuccess: () => {
      pushToast('Backup dihapus.', 'success')
      setDeletingBackup(null)
      void queryClient.invalidateQueries({ queryKey: ['backups'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const watchdogMutation = useMutation({
    mutationFn: () =>
      updateWatchdogSettings({
        interval_minutes: Math.min(60, Math.max(1, Number(wdInterval) || 5)),
        auto_restart: wdAutoRestart,
      }),
    onSuccess: () => {
      pushToast('Pengaturan watchdog disimpan.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['watchdog', 'status'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const autoBackupMutation = useMutation({
    mutationFn: () => {
      const body: AutoBackupSettings = {
        enabled: abEnabled,
        frequency: abFrequency,
        hour: Math.min(23, Math.max(0, Number(abHour) || 2)),
        retention: Math.max(1, Number(abRetention) || 7),
      }
      return updateAutoBackupSettings(body)
    },
    onSuccess: () => {
      pushToast('Pengaturan auto-backup disimpan.', 'success')
      void queryClient.invalidateQueries({ queryKey: ['auto-backup-settings'] })
    },
    onError: (e: Error) => pushToast(e.message, 'error'),
  })

  const backups = backupsQuery.data ?? []

  return (
    <div className="space-y-4">
      <PageHeader title="Settings" subtitle="Koneksi backend, tampilan, dan backup" />

      <Card>
        <h2 className="mb-3 font-bold">Koneksi Backend</h2>
        <div className="space-y-3">
          <label className="block text-sm">
            <span className="mb-1 block font-medium">Base URL backend</span>
            <input
              value={urlDraft}
              onChange={(e) => setUrlDraft(e.target.value)}
              placeholder={DEFAULT_BASE_URL}
              inputMode="url"
              className={inputCls}
            />
          </label>
          <label className="block text-sm">
            <span className="mb-1 block font-medium">API Key</span>
            <div className="flex gap-2">
              <input
                type={showKey ? 'text' : 'password'}
                value={keyDraft}
                onChange={(e) => setKeyDraft(e.target.value)}
                placeholder="API key server (header X-API-Key)"
                autoComplete="off"
                className={inputCls}
              />
              <button
                type="button"
                onClick={() => setShowKey((s) => !s)}
                className="shrink-0 rounded-full bg-ink/10 px-3 py-1.5 text-xs font-semibold dark:bg-darktext/10"
              >
                {showKey ? 'Sembunyi' : 'Tampil'}
              </button>
            </div>
          </label>
          <p className="rounded-xl bg-amber-500/15 p-3 text-sm text-amber-700 dark:text-amber-400">
            Peringatan: URL dan API key disimpan di localStorage perangkat ini. Jangan bagikan perangkat atau
            tangkapan layar yang menampilkan API key.
          </p>
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              onClick={() => {
                setBaseUrl(urlDraft.trim() || DEFAULT_BASE_URL)
                setApiKey(keyDraft.trim())
                pushToast('Pengaturan koneksi disimpan.', 'success')
              }}
              className="rounded-full bg-primary px-4 py-2 text-sm font-bold text-white"
            >
              Simpan
            </button>
            <button
              type="button"
              onClick={() => testMutation.mutate()}
              disabled={testMutation.isPending}
              className="flex items-center gap-1 rounded-full bg-ink/10 px-4 py-2 text-sm font-bold dark:bg-darktext/10"
            >
              <RefreshCw className="h-4 w-4" />
              {testMutation.isPending ? 'Menguji…' : 'Tes Koneksi'}
            </button>
          </div>
          {testMutation.isSuccess && testMutation.data && (
            <p className="flex items-center gap-1 text-sm font-medium text-success">
              <CheckCircle2 className="h-4 w-4" />
              Terhubung — status: {testMutation.data.status ?? 'ok'}, DB: {testMutation.data.db ?? '—'},
              versi: {testMutation.data.version ?? '—'}
            </p>
          )}
          {testMutation.isError && (
            <p className="flex items-center gap-1 text-sm font-medium text-danger">
              <XCircle className="h-4 w-4" /> {(testMutation.error as Error).message}
            </p>
          )}
        </div>
      </Card>

      <Card>
        <h2 className="mb-3 font-bold">Tampilan & Bahasa</h2>
        <div className="space-y-3">
          <div className="flex items-center justify-between gap-3">
            <span className="text-sm font-medium">Tema</span>
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => handleTheme('light')}
                className={`rounded-full px-3 py-1.5 text-sm font-semibold ${theme === 'light' ? 'bg-primary text-white' : 'bg-ink/10 dark:bg-darktext/10'}`}
              >
                Terang
              </button>
              <button
                type="button"
                onClick={() => handleTheme('dark')}
                className={`rounded-full px-3 py-1.5 text-sm font-semibold ${theme === 'dark' ? 'bg-primary text-white' : 'bg-ink/10 dark:bg-darktext/10'}`}
              >
                Gelap
              </button>
            </div>
          </div>
          <div className="flex items-center justify-between gap-3">
            <span className="text-sm font-medium">Bahasa</span>
            <Badge tone="info">Bahasa Indonesia</Badge>
          </div>
          {serverSettingsQuery.data && (
            <p className="text-xs text-ink/50 dark:text-darktext/50">
              Pengaturan server tersinkron: tema server{' '}
              {String(serverSettingsQuery.data.theme ?? '—')}.
            </p>
          )}
        </div>
      </Card>

      <AiProvidersSection />

      <Card>
        <h2 className="mb-3 flex items-center gap-2 font-bold">
          <HeartPulse className="h-5 w-5" /> Bot Watchdog
        </h2>
        <p className="mb-3 text-sm text-ink/60 dark:text-darktext/60">
          Watchdog memeriksa koneksi setiap bot secara berkala dan bisa me-restart bot yang gagal.
        </p>
        {watchdogQuery.isLoading ? (
          <LoadingSpinner label="Memuat pengaturan watchdog…" />
        ) : watchdogQuery.isError ? (
          <ErrorState message={(watchdogQuery.error as Error).message} onRetry={() => void watchdogQuery.refetch()} />
        ) : (
          <div className="space-y-3">
            <label className="block text-sm">
              <span className="mb-1 block font-medium">Interval pengecekan (menit, 1–60)</span>
              <input
                value={wdInterval}
                onChange={(e) => setWdInterval(e.target.value)}
                inputMode="numeric"
                className={inputCls}
              />
            </label>
            <label className="flex items-center justify-between gap-3">
              <span className="text-sm font-medium">Auto-restart bot yang gagal</span>
              <input
                type="checkbox"
                checked={wdAutoRestart}
                onChange={(e) => setWdAutoRestart(e.target.checked)}
                className="h-5 w-5 accent-primary"
              />
            </label>
            <button
              type="button"
              onClick={() => watchdogMutation.mutate()}
              disabled={watchdogMutation.isPending}
              className="rounded-full bg-primary px-4 py-2 text-sm font-bold text-white disabled:opacity-50"
            >
              {watchdogMutation.isPending ? 'Menyimpan…' : 'Simpan Pengaturan Watchdog'}
            </button>
          </div>
        )}
      </Card>

      <Card>
        <h2 className="mb-3 flex items-center gap-2 font-bold">
          <Database className="h-5 w-5" /> Backup & Restore
        </h2>
        <label className="mb-3 block text-sm">
          <span className="mb-1 block font-medium text-ink/70 dark:text-darktext/70">
            Passphrase backup (min. 8 karakter)
          </span>
          <input
            type="password"
            value={backupPassphrase}
            onChange={(e) => setBackupPassphrase(e.target.value)}
            placeholder="Dipakai untuk mengenkripsi backup baru dan restore backup"
            className={inputCls}
            autoComplete="off"
          />
          <span className="mt-1 block text-xs text-ink/50 dark:text-darktext/50">
            Backup dienkripsi AES-256-GCM dengan passphrase ini. Restore membutuhkan passphrase yang
            sama — simpan baik-baik, passphrase tidak disimpan di perangkat.
          </span>
        </label>
        <button
          type="button"
          onClick={() => createBackupMutation.mutate()}
          disabled={!passphraseOk || createBackupMutation.isPending}
          className="mb-3 flex items-center gap-1 rounded-full bg-primary px-4 py-2 text-sm font-bold text-white disabled:opacity-50"
        >
          <HardDriveDownload className="h-4 w-4" />
          {createBackupMutation.isPending ? 'Membuat…' : 'Buat Backup'}
        </button>
        {backupsQuery.isLoading ? (
          <LoadingSpinner label="Memuat daftar backup…" />
        ) : backupsQuery.isError ? (
          <ErrorState message={(backupsQuery.error as Error).message} onRetry={() => void backupsQuery.refetch()} />
        ) : backups.length === 0 ? (
          <EmptyState title="Belum ada backup" description="Backup menyimpan konfigurasi dan data TeleCore di server." />
        ) : (
          <ul className="space-y-2">
            {backups.map((b) => (
              <li key={String(b.id)} className="flex items-center justify-between gap-3 rounded-xl border border-ink/10 p-3 dark:border-darktext/10">
                <div className="min-w-0 text-sm">
                  <p className="flex flex-wrap items-center gap-1.5 truncate font-semibold">
                    {b.name ?? b.filename ?? `Backup ${b.id}`}
                    {(b.is_automatic === true || b.is_automatic === 1) && (
                      <Badge tone="info">Otomatis</Badge>
                    )}
                  </p>
                  <p className="text-ink/50 dark:text-darktext/50">
                    {b.created_at ? new Date(b.created_at).toLocaleString('id-ID') : ''}
                    {(b.size ?? b.size_bytes) !== undefined
                      ? ` · ${(((b.size ?? b.size_bytes) as number) / 1024).toFixed(1)} KB`
                      : ''}
                  </p>
                </div>
                <div className="flex shrink-0 gap-1">
                  <button
                    type="button"
                    onClick={() => setRestoring(b)}
                    disabled={!passphraseOk}
                    title={passphraseOk ? undefined : 'Isi passphrase backup (min. 8 karakter) dulu'}
                    className="flex items-center gap-1 rounded-full bg-primary/10 px-3 py-1.5 text-xs font-semibold text-primary disabled:opacity-50"
                  >
                    <Download className="h-3.5 w-3.5" /> Restore
                  </button>
                  <button
                    type="button"
                    aria-label="Hapus backup"
                    onClick={() => setDeletingBackup(b)}
                    className="rounded-full p-1.5 text-danger hover:bg-danger/10"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}

        <div className="mt-4 space-y-3 border-t border-ink/10 pt-4 dark:border-darktext/10">
          <h3 className="font-bold">Auto-Backup Terjadwal</h3>
          {autoBackupQuery.isLoading ? (
            <LoadingSpinner label="Memuat pengaturan auto-backup…" />
          ) : autoBackupQuery.isError ? (
            <ErrorState message={(autoBackupQuery.error as Error).message} onRetry={() => void autoBackupQuery.refetch()} />
          ) : (
            <>
              <label className="flex items-center justify-between gap-3">
                <span className="text-sm font-medium">Auto-backup aktif</span>
                <input
                  type="checkbox"
                  checked={abEnabled}
                  onChange={(e) => setAbEnabled(e.target.checked)}
                  className="h-5 w-5 accent-primary"
                />
              </label>
              <div className="grid grid-cols-3 gap-3">
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Frekuensi</span>
                  <select
                    value={abFrequency}
                    onChange={(e) => setAbFrequency(e.target.value as 'daily' | 'weekly')}
                    className={inputCls}
                  >
                    <option value="daily">Harian</option>
                    <option value="weekly">Mingguan</option>
                  </select>
                </label>
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Jam (0–23)</span>
                  <input value={abHour} onChange={(e) => setAbHour(e.target.value)} inputMode="numeric" className={inputCls} />
                </label>
                <label className="block text-sm">
                  <span className="mb-1 block font-medium">Retensi (generasi)</span>
                  <input value={abRetention} onChange={(e) => setAbRetention(e.target.value)} inputMode="numeric" className={inputCls} />
                </label>
              </div>
              <p className="text-xs text-ink/50 dark:text-darktext/50">
                Backup otomatis dibuat di server sesuai jadwal dan ditandai badge &quot;Otomatis&quot;
                di daftar di atas. Generasi lama di luar retensi dihapus otomatis.
              </p>
              <button
                type="button"
                onClick={() => autoBackupMutation.mutate()}
                disabled={autoBackupMutation.isPending}
                className="rounded-full bg-primary px-4 py-2 text-sm font-bold text-white disabled:opacity-50"
              >
                {autoBackupMutation.isPending ? 'Menyimpan…' : 'Simpan Auto-Backup'}
              </button>
            </>
          )}
        </div>
      </Card>

      <Card>
        <h2 className="mb-2 font-bold">Tentang</h2>
        <dl className="space-y-1 text-sm">
          <div className="flex justify-between">
            <dt className="text-ink/60 dark:text-darktext/60">Aplikasi</dt>
            <dd className="font-medium">TeleCore v1.3.0</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-ink/60 dark:text-darktext/60">Backend</dt>
            <dd className="font-medium">Rust (Axum) — berjalan di server/PC/Termux, bukan di dalam aplikasi</dd>
          </div>
          <div className="flex justify-between">
            <dt className="text-ink/60 dark:text-darktext/60">Biometrik</dt>
            <dd className="font-medium">Tidak tersedia di v1 (hanya PIN)</dd>
          </div>
        </dl>
      </Card>

      <ConfirmDialog
        open={restoring !== null}
        onClose={() => setRestoring(null)}
        onConfirm={() => restoring && restoreMutation.mutate(restoring.id)}
        title="Restore Backup"
        message={`Pulihkan backup "${restoring?.name ?? restoring?.filename ?? ''}"? Data saat ini di server akan diganti dengan isi backup.`}
        confirmLabel="Restore"
        busy={restoreMutation.isPending}
      />
      <ConfirmDialog
        open={deletingBackup !== null}
        onClose={() => setDeletingBackup(null)}
        onConfirm={() => deletingBackup && deleteBackupMutation.mutate(deletingBackup.id)}
        title="Hapus Backup"
        message={`Hapus backup "${deletingBackup?.name ?? deletingBackup?.filename ?? ''}" secara permanen?`}
        busy={deleteBackupMutation.isPending}
      />
    </div>
  )
}
