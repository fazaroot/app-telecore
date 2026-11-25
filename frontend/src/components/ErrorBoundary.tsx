import { Component, type ReactNode } from 'react'

/**
 * Penangkap error render React. Tanpa ini, satu exception saat render
 * (mis. dereferensi null di sebuah halaman) membuat seluruh aplikasi
 * unmount dan layar jadi putih polos tanpa pesan apa pun.
 * Boundary ini menampilkan layar error yang bisa dipulihkan, bukan putih.
 */
export class ErrorBoundary extends Component<
  { children: ReactNode },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null }

  static getDerivedStateFromError(error: Error) {
    return { error }
  }

  componentDidCatch(error: Error) {
    // Dicatat ke console agar bisa dilaporkan; tidak dikirim ke mana pun.
    console.error('[TeleCore] Render error:', error)
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex min-h-screen flex-col items-center justify-center gap-3 bg-bg px-6 text-center dark:bg-darkbg">
          <p className="text-lg font-bold text-ink dark:text-darktext">Terjadi kesalahan</p>
          <p className="max-w-sm text-sm text-ink/70 dark:text-darktext/70">
            Halaman ini gagal ditampilkan ({this.state.error.message}). Data Anda aman — coba muat
            ulang aplikasi.
          </p>
          <button
            type="button"
            onClick={() => window.location.reload()}
            className="rounded-full bg-primary px-5 py-2.5 text-sm font-bold text-white"
          >
            Muat Ulang Aplikasi
          </button>
        </div>
      )
    }
    return this.props.children
  }
}
