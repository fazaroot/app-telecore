# TeleCore — Telegram Bot Control Center

Platform manajemen, konfigurasi, monitoring, dan otomatisasi Telegram Bot:
multi-bot, messaging, automation engine, scheduler, analytics, notifikasi,
security center, dan backup/restore — dengan backend Rust dan aplikasi Android
(Capacitor).

## Arsitektur v1 — baca dulu (jujur)

TeleCore terdiri dari **dua bagian yang berjalan terpisah**:

1. **Backend Rust** (`backend/`) — server binary asli (Axum REST + WebSocket +
   SQLite) yang berjalan di **server/VPS/PC/Termux**. Inilah yang berbicara
   dengan Telegram Bot API, menjalankan long polling, automation engine, dan
   scheduler.
2. **Aplikasi Android** (`frontend/` + `android/`) — frontend React yang
   dibungkus Capacitor. APK **tidak** berisi backend. Aplikasi terhubung ke
   backend lewat URL yang bisa dikonfigurasi di **Settings** (default
   `http://127.0.0.1:8080`, ubah ke IP LAN/VPS tempat backend berjalan).

> **Batasan yang diketahui:** menanam server Axum penuh ke dalam APK via JNI
> **tidak dicoba di v1** karena kompleksitas dan ketidakandalan yang tinggi.
> Backend harus berjalan terpisah agar bot bisa menerima pesan, automation
> berjalan, dan scheduler mengeksekusi task. Saat backend tidak terjangkau,
> aplikasi menampilkan status offline yang jelas dan data terakhir yang
> tersimpan (dengan penanda data lama); pengaturan lokal dan draft tetap bisa
> dilihat.

## Fitur

- **Dashboard** — total bot, bot aktif/nonaktif, pesan masuk/keluar, total
  pengguna, status automation/API/server/DB, error monitoring, grafik traffic,
  live activity feed via WebSocket.
- **My Bots** — tambah bot via token (validasi `getMe`), edit, hapus,
  aktif/nonaktif, uji koneksi, webhook & long polling (saling eksklusif),
  export/import konfigurasi (token diekspor **terenkripsi**, bukan plaintext).
- **Messages** — kirim teks, foto, dokumen, video, audio, animasi (file dari
  perangkat), lokasi, kontak, media group (via API), inline/reply keyboard
  builder, MarkdownV2/HTML + preview, history, status pengiriman, retry pesan
  teks yang gagal, template lokal perangkat.
- **Automation** — workflow builder berbasis langkah (trigger → kondisi →
  aksi, multi-step + delay), welcome message, auto-reply, keyword, command,
  history eksekusi, enable/disable per workflow, loop guard (maks 10 eksekusi
  per workflow per menit, maks 20 langkah, pesan dari bot sendiri diabaikan).
- **Users** — daftar pengguna, profil, search/filter, notes, label,
  block/unblock, export CSV.
- **Groups** — info grup, pengaturan welcome/keyword filter/anti-spam/link
  filter/flood/warning, moderation log. Aksi moderasi **hanya** berjalan bila
  bot adalah admin grup (dicek via `getChatMember`); bila bukan admin,
  aplikasi menampilkan peringatan dan tidak mengeksekusi moderasi.
- **Scheduler** — one-time, harian, mingguan, interval, cron
  (`"MENIT JAM * * DOW"`), timezone, pause/resume, history, retry gagal
  (maks 3x lalu dinonaktifkan + notifikasi).
- **Analytics** — pesan per hari (inbound/outbound), pengguna aktif/baru,
  performa automation, failed requests, command usage, filter tanggal, export
  CSV/JSON, cetak/PDF via print browser.
- **Notifications** — bot disconnect, error API, automation gagal, task
  selesai/gagal, aktivitas tidak biasa; event penting memicu notifikasi lokal
  Android.
- **Security** — token bot dienkripsi AES-256-GCM, token tidak pernah
  ditampilkan lengkap (hanya 4 karakter terakhir), PIN app lock (hash SHA-256
  + salt; kunci level aplikasi, bukan hardware-backed), API key, rate limit,
  redaksi secret di log, session management, auto-lock. **Biometrik tidak ada
  di v1** (hanya PIN).
- **Backup & Restore** — backup SQLite terenkripsi AES-256-GCM dengan
  passphrase, validasi sebelum restore (dekripsi + header SQLite +
  `integrity_check`), history backup.

## Struktur proyek

```
TeleCore/
├── backend/            Rust (Axum, Tokio, SQLx, SQLite)
│   ├── src/            main, config, api (14 handler), bot, automation,
│   │                   scheduler, security, websocket, services
│   └── migrations/     15 tabel + FK + index
├── frontend/           React + TypeScript + Vite + Tailwind v3
│   └── src/            11 halaman, api client, stores (Zustand),
│                       hooks (WebSocket, notifikasi lokal), komponen
├── android/            Proyek Capacitor Android (applicationId com.telecore.app)
├── .github/workflows/  android-build.yml (CI: backend + frontend + APK debug)
├── docs/               ANDROID.md, SIGNING.md
├── scripts/            (kosong — skrip utilitas bila diperlukan)
├── capacitor.config.ts, package.json (root, khusus Capacitor)
├── .env.example, .gitignore, LICENSE (MIT)
└── README.md
```

## Menjalankan backend (server/PC/Termux)

Persyaratan: Rust stable (cargo), tidak perlu Android SDK.

```bash
cd backend
cp ../.env.example ../.env   # isi TELECORE_API_KEY + TELECORE_ENCRYPTION_KEY
# TELECORE_ENCRYPTION_KEY: 32 byte hex (64 karakter), mis. hasil `openssl rand -hex 32`
# Bila tidak diisi, server generate sekali dan simpan di telecore.key (mode 0600).
cargo run --release
# Server berjalan di http://127.0.0.1:8080 (atur TELECORE_HOST/PORT bila perlu)
```

Variabel lingkungan: `TELECORE_HOST`, `TELECORE_PORT` (default 8080),
`DATABASE_URL` (default `sqlite:telecore.db`), `TELECORE_API_KEY` (wajib untuk
semua `/api/*` kecuali `/api/health`, lewat header `X-API-Key`),
`TELECORE_ENCRYPTION_KEY`, `RUST_LOG`.

API: envelope konsisten `{"success": true, "data": ...}` /
`{"success": false, "error": {"code": "...", "message": "..."}}`.
WebSocket live update di `/ws`. Webhook Telegram di
`POST /api/webhooks/telegram/{bot_id}`. Daftar endpoint lengkap:
[backend/README.md](backend/README.md).

## Menjalankan frontend (pengembangan web)

```bash
cd frontend
npm install
npm run dev
```

Atur Backend URL + API key di halaman Settings aplikasi.

## Mendapatkan APK

APK **hanya** dibangun oleh GitHub Actions (tidak ada build lokal — tidak ada
Android SDK/JDK di lingkungan ini):

1. Buat repo GitHub baru (nama belum diputuskan — mis. `fazaroot/app-telecore`,
   menunggu keputusan Astr) dan push seluruh isi proyek ini ke branch `main`.
2. Workflow `.github/workflows/android-build.yml` berjalan otomatis: job
   `backend` (cargo test + build release, artifact binary server), job
   `frontend` (vitest + build), job `android` (build frontend, `cap sync`,
   Gradle `assembleDebug`, validasi APK > 1 MB).
3. Unduh **`telecore-debug.apk`** dari Artifacts di halaman Actions run,
   lalu instal di HP (izinkan "instal dari sumber tidak dikenal" bila diminta).
4. Di aplikasi, buka **Settings** → isi Backend URL (IP LAN/VPS tempat backend
   berjalan, mis. `http://192.168.1.10:8080`) + API key yang sama dengan
   `TELECORE_API_KEY` di server → Tes Koneksi.

> **APK belum pernah dibangun.** Workflow belum pernah berjalan di GitHub
> karena repo belum dibuat dan belum ada push. Jangan menganggap APK tersedia
> sebelum artifact `telecore-debug-apk` benar-benar muncul di Actions.
> Release APK (signed) belum tersedia — panduan signing ada di
> [docs/SIGNING.md](docs/SIGNING.md).

Detail Android (permission, cleartext HTTP lokal, SDK 35): [docs/ANDROID.md](docs/ANDROID.md).

## Hasil verifikasi (2026-11-25, lingkungan Ubuntu 24.04)

Angka-angka ini adalah hasil pengecekan nyata, bukan klaim:

| Pemeriksaan | Hasil |
|---|---|
| `cargo test` (backend) | **41 passed, 0 failed** (enkripsi/masking, parsing Telegram, kondisi automation + loop guard, scheduler incl. cron harian/mingguan, validasi API, serialisasi kontrak API) |
| `cargo build` (backend) | Lolos (binary `telecore-backend`) |
| `npx tsc --noEmit` (frontend) | **0 error** |
| `npm test` / vitest (frontend) | **8 passed, 0 failed** |
| `npm run build` (frontend) | Lolos (`dist/` ~880 KB) |
| `npx cap sync android` | Lolos, 8 plugin Capacitor terdeteksi |
| Smoke test API (23 cek, server sementara + DB seed) | **Semua lolos** — health, auth 401, kirim teks/media payload persis frontend (mencapai api.telegram.org, 401 karena token palsu — membuktikan jalur kirim utuh), webhook tanpa `action`, workflow + eksekusi, scheduler + cron harian (next run 09:00 sesuai jam yang dipilih), command + uji, backup/restore passphrase (benar OK, salah 400), analytics, notifikasi, grup, retry |
| Alur webhook end-to-end | Update Telegram palsu → pesan inbound tersimpan → workflow keyword terpicu → eksekusi tercatat (gagal kirim karena token palsu, sesuai harapan) |

Perbaikan integrasi yang dilakukan saat verifikasi (ditemukan karena frontend
dan backend dibangun terpisah lalu dicocokkan): serialisasi API (field JSON
sebagai objek/array, flag 0/1 sebagai boolean, `status` → `connection_status`),
payload kirim pesan frontend (`POST /api/messages`, `chat_id` string angka,
`message_type`/`keyboard`/media data URL), route yang hilang
(`PUT /groups/{id}`, `/automation/{id}/executions`, `/scheduler/{id}/history`),
passphrase backup di UI, pemetaan field scheduler/command/automation, bug
`FromRow` grup/notifikasi (kolom `type`), scheduler harian/mingguan yang
mengabaikan jam cron, dan tombol inline keyboard dengan `url` + `callback_data`
bersamaan (ditolak Telegram).

## Batasan yang diketahui (v1)

- **Belum diuji dengan bot Telegram asli** — tidak ada token bot pengujian.
  Semua panggilan Telegram terverifikasi sampai ke api.telegram.org dengan
  token palsu (401), dan alur webhook/automation terverifikasi dengan update
  palsu. Pengujian live dengan bot test adalah langkah Astr di perangkat.
- **APK belum pernah dibangun** (menunggu repo + push + CI).
- `avg_response_time_ms` di analytics = `null` (belum dilacak).
- Retry pesan hanya untuk pesan teks (konten media tidak tersimpan di server).
- Cron hanya format `"MENIT JAM * * DOW"` (tanpa `*/n`, range, atau list).
- Kirim pesan hanya ke **ID chat numerik** (ID grup negatif diizinkan);
  username `@...` belum didukung di v1.
- Template pesan tersimpan **lokal di perangkat** (bukan di server).
- `/ws` tanpa autentikasi API key.
- `compileSdk`/`targetSdk` Android = 35 (bukan 34) karena Capacitor 7 +
  AndroidX 1.15.0 mewajibkannya.
- Batas body JSON 55 MB (untuk kirim media via data URL); file media maks
  50 MB (batas Bot API).

## Keamanan

- Jangan pernah commit `.env`, `*.db`, `telecore.key`, atau backup `*.enc`
  (sudah di `.gitignore`). `.env.example` hanya berisi placeholder.
- Token bot disimpan terenkripsi (AES-256-GCM); tidak pernah muncul lengkap
  di API, log, atau export.
- API key dari env; ganti dengan mengubah env lalu restart server.

## Lisensi

MIT — lihat [LICENSE](LICENSE).
