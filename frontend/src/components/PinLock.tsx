import { useState } from 'react'
import { Lock } from 'lucide-react'
import { usePinStore, verifyPinLocal } from '../stores/pin'

export function PinLockScreen() {
  const [pin, setPin] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const unlock = usePinStore((s) => s.unlock)

  const submit = async () => {
    if (pin.length < 4) {
      setError('PIN minimal 4 digit.')
      return
    }
    setBusy(true)
    setError('')
    try {
      const ok = await verifyPinLocal(pin)
      if (ok) {
        unlock()
      } else {
        setError('PIN salah. Coba lagi.')
        setPin('')
      }
    } finally {
      setBusy(false)
    }
  }

  const pressDigit = (d: string) => {
    if (pin.length >= 6) return
    setError('')
    setPin((p) => p + d)
  }

  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-bg px-6 dark:bg-darkbg">
      <Lock className="mb-4 h-12 w-12 text-primary" />
      <h1 className="text-xl font-bold text-ink dark:text-darktext">TeleCore terkunci</h1>
      <p className="mt-1 text-center text-sm text-ink/60 dark:text-darktext/60">
        Masukkan PIN aplikasi untuk membuka.
      </p>
      <div className="mt-6 flex gap-3" aria-label="PIN">
        {Array.from({ length: 6 }).map((_, i) => (
          <span
            key={i}
            className={`h-3.5 w-3.5 rounded-full border-2 ${
              i < pin.length ? 'border-primary bg-primary' : 'border-ink/30 dark:border-darktext/30'
            }`}
          />
        ))}
      </div>
      {error && <p className="mt-3 text-sm font-medium text-danger">{error}</p>}
      <div className="mt-6 grid w-full max-w-[240px] grid-cols-3 gap-3">
        {['1', '2', '3', '4', '5', '6', '7', '8', '9'].map((d) => (
          <button
            key={d}
            type="button"
            onClick={() => pressDigit(d)}
            className="rounded-full bg-surface py-3 text-lg font-semibold text-ink shadow-sm dark:bg-darksurface dark:text-darktext"
          >
            {d}
          </button>
        ))}
        <button
          type="button"
          onClick={() => setPin((p) => p.slice(0, -1))}
          className="rounded-full bg-surface py-3 text-sm font-semibold text-ink shadow-sm dark:bg-darksurface dark:text-darktext"
        >
          Hapus
        </button>
        <button
          type="button"
          onClick={() => pressDigit('0')}
          className="rounded-full bg-surface py-3 text-lg font-semibold text-ink shadow-sm dark:bg-darksurface dark:text-darktext"
        >
          0
        </button>
        <button
          type="button"
          onClick={submit}
          disabled={busy}
          className="rounded-full bg-primary py-3 text-sm font-semibold text-white disabled:opacity-50"
        >
          Buka
        </button>
      </div>
      <p className="mt-8 max-w-xs text-center text-xs text-ink/50 dark:text-darktext/50">
        Kunci level aplikasi — hanya hash PIN yang disimpan di perangkat ini. Bukan kunci
        hardware-backed dan bukan pengganti biometrik (biometrik tidak tersedia di v1).
      </p>
    </div>
  )
}
