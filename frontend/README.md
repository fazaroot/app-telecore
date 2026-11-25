# TeleCore Frontend

Frontend React untuk **TeleCore — Telegram Bot Control Center**. Mobile-first,
dipakai di browser dan sebagai WebView di dalam APK Capacitor.

## Stack

- React 18 + TypeScript + Vite
- Tailwind CSS v3 + PostCSS + autoprefixer
- Zustand (state), TanStack Query (data fetching), Recharts, Framer Motion,
  Lucide React, react-router-dom
- @capacitor/core + plugin resmi (@capacitor/app, status-bar, splash-screen,
  local-notifications, filesystem, share, network, preferences)

## Cara menjalankan

```bash
npm install --no-audit --no-fund
npm run dev        # dev server di http://localhost:5173
npm run build      # typecheck + build produksi ke dist/
npm test           # vitest (8 test)
```

## Konfigurasi backend

Backend Rust (Axum) **tidak** berjalan di dalam aplikasi ini — ia berjalan di
server/PC/Termux dan dihubungi lewat HTTP + WebSocket. Atur alamatnya di
halaman **Settings** di aplikasi (default `http://127.0.0.1:8080`) beserta API
key (header `X-API-Key`). Keduanya disimpan di localStorage perangkat.

## Catatan jujur

- **Template pesan adalah template lokal perangkat.** Backend v1 belum
  menyediakan endpoint template terpisah, jadi template disimpan di
  localStorage dan tidak tersinkron ke server. UI memberi label ini dengan
  jelas.
- **PIN / biometrik.** v1 hanya memakai PIN 4–6 digit. PIN dikirim ke backend
  untuk di-hash di server (`POST /api/security/change-pin`); app lock lokal
  menyimpan hanya hash SHA-256 + salt via @capacitor/preferences. Ini kunci
  level aplikasi, bukan hardware-backed. Biometrik tidak tersedia di v1 dan
  tidak ada tombol biometrik di UI.
- **Automation builder berbasis langkah** (pilih trigger → tambah kondisi →
  tambah action), bukan drag-drop.
- **Kalender Scheduler** berupa daftar task yang dikelompokkan per tanggal,
  bukan kalender drag-drop.
- **Tombol "Cetak / Simpan PDF"** di Analytics memakai `window.print()` +
  stylesheet print (via print browser).
- **Media composer** memakai input file web standar; di WebView Capacitor hal
  ini juga berfungsi. Plugin @capacitor/filesystem tersedia sebagai dependensi
  untuk kebutuhan native lanjutan.
- **Notifikasi lokal** (LocalNotifications) hanya aktif di platform native;
  di web, pemanggilan dibungkus try/catch dan diabaikan dengan aman.
- Chunk JS utama ~871 kB (gzip ~248 kB) — Vite memberi peringatan ukuran
  chunk, bukan error. Code-splitting bisa ditambahkan nanti bila perlu.
