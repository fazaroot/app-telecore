// Sinkronisasi StatusBar native (Android) dengan tema aplikasi.
// Dipanggil sekali saat start dan setiap kali tema berubah.
// - overlaysWebView: false → WebView tidak menutupi status bar, jadi
//   env(safe-area-inset-top) bernilai 0 dan tidak ada padding ganda.
// - Tema gelap → ikon status bar terang (Style.Light), background header gelap.
// - Tema terang → ikon status bar gelap (Style.Dark), background header terang.
// Aman di preview web: guard isNativePlatform + try/catch.
import { Capacitor } from '@capacitor/core'
import { StatusBar, Style } from '@capacitor/status-bar'

export async function syncStatusBar(theme: 'light' | 'dark'): Promise<void> {
  try {
    if (!Capacitor.isNativePlatform()) return
    await StatusBar.setOverlaysWebView({ overlay: false })
    await StatusBar.setStyle({ style: theme === 'dark' ? Style.Light : Style.Dark })
    await StatusBar.setBackgroundColor({ color: theme === 'dark' ? '#1A2233' : '#FFFFFF' })
  } catch {
    // StatusBar tidak tersedia (mis. preview web) — abaikan.
  }
}
