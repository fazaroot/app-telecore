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

## Yang baru di v1.1.0

Semua fitur di bawah ini **gratis, tanpa paywall di mana pun** — tidak ada
fitur berbayar di TeleCore.

- **Perbaikan header Android (bug POCO M6)** — header aplikasi sebelumnya
  mentok ke status bar sehingga tombol hamburger tidak bisa diklik. Kini
  `StatusBar.overlaysWebView: false` (config + runtime, mengikuti tema
  light/dark) dan header/drawer/PIN lock memakai padding
  `env(safe-area-inset-top)`. Perbaikan ini terverifikasi di build, **belum
  di perangkat** — konfirmasi akhir ada di tangan Astr di POCO M6.
- **MCP server** — endpoint `POST /mcp` (JSON-RPC 2.0, Streamable HTTP,
  auth `X-API-Key`): `initialize`, `ping`, `tools/list`, `tools/call` dengan
  7 tools (`list_bots`, `get_bot`, `send_message`, `list_users`,
  `get_analytics_summary`, `list_workflows`, `list_scheduled_tasks`).
  Halaman **Integrations** menampilkan endpoint, tombol tes koneksi, dan
  salin konfigurasi JSON untuk Claude Desktop. Lihat [docs/MCP.md](docs/MCP.md).
- **Outgoing webhooks** — langganan event (`message.inbound`,
  `automation.executed`, `bot.disconnected`, `scheduled_task.completed`),
  signature HMAC-SHA256 (`X-TeleCore-Signature`), log pengiriman, retry
  manual, tombol "Kirim uji".
- **Personal API tokens** — buat/revoke token di Security; token tampil penuh
  **hanya sekali** saat dibuat, disimpan sebagai hash SHA-256, dipakai sebagai
  `X-API-Key`.
- **Broadcast messaging** — kirim satu pesan ke banyak user/grup sekaligus,
  progress live + status per penerima (terkirim/gagal).
- **Template variables** — `{{first_name}}`, `{{username}}`,
  `{{chat_title}}` disubstitusi server saat pengiriman, dengan preview hasil
  render di composer.
- **Message search** — pencarian riwayat pesan (`GET /api/messages/search`).
- **Live Logs** — halaman Logs: 200 baris log server terakhir, live via
  WebSocket (fallback polling 5 detik).
- **Per-bot analytics** — breakdown statistik per bot + export CSV.
- Versi: backend `1.3.0` (`/api/health`), aplikasi `TeleCore v1.3.0`.
- **v1.2.1** — perbaikan bug halaman My Bots blank putih: form AI di modal
  pengaturan bot dereferensi `configBot` yang masih null saat halaman
  di-render. Ditambah `ErrorBoundary` agar error render menampilkan layar
  error yang bisa dipulihkan, bukan layar putih.

## Yang baru di v1.3.0 — CRM + manajemen grup

Semua fitur di bawah ini **gratis, tanpa paywall di mana pun** — tidak ada
fitur berbayar di TeleCore.

- **CRM — Tag** — tag berwarna di tiap pengguna (badge warna di kartu user),
  buat tag + pilih warna, hapus tag, pasang/lepas tag dari detail pengguna,
  filter daftar pengguna berdasarkan tag. Export CSV kini menyertakan kolom tag.
- **CRM — Segmen** — halaman baru **Segmen** (drawer): buat segmen dengan
  pembangun kriteria (tag, sudah-pernah-order, aktif ≤ N hari terakhir,
  tidak-aktif > N hari, minimal jumlah pesan) + **preview jumlah anggota**
  sebelum disimpan; klik segmen → daftar anggota; **follow-up** kirim pesan ke
  semua anggota segmen dengan hasil terkirim/gagal; edit/hapus segmen; filter
  daftar pengguna berdasarkan segmen. Lihat [docs/CRM.md](docs/CRM.md).
- **CRM — Timeline & catatan** — detail pengguna kini menampilkan timeline
  kronologis (pesan, order, catatan, tag) dan riwayat catatan (tambah/hapus).
- **Manajemen grup** — pengaturan per grup: welcome on/off + pesan dengan
  template `{{first_name}}` `{{chat_title}}`, anti-spam + batas flood +
  window (detik), block links, kata terlarang (tambah/hapus), aksi pelanggaran
  (hapus/peringatan/mute) + durasi mute. Aksi moderasi manual pada anggota
  (mute/ban dengan durasi + alasan, unmute/unban), kartu statistik (anggota,
  pesan hari ini/7 hari, user aktif, pengirim teratas), dan log moderasi.
  Lihat [docs/GROUPS.md](docs/GROUPS.md).
- **Pelajaran v1.2.1 ditegakkan** — semua modal/dialog baru di-guard
  (`{x !== null && <Form .../>}`) agar tidak pernah dereferensi state nullable
  saat parent di-render; ada regression test render-dengan-modal-tertutup untuk
  Users, Groups, dan Segments (`frontend/src/test/v13.test.tsx`).

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
  teks yang gagal, template lokal perangkat, **broadcast ke banyak penerima**,
  **template variables** (`{{first_name}}`, `{{username}}`,
  `{{chat_title}}`) dengan preview render server, **pencarian riwayat pesan**.
- **Automation** — workflow builder berbasis langkah (trigger → kondisi →
  aksi, multi-step + delay), welcome message, auto-reply, keyword, command,
  history eksekusi, enable/disable per workflow, loop guard (maks 10 eksekusi
  per workflow per menit, maks 20 langkah, pesan dari bot sendiri diabaikan).
- **Users** — daftar pengguna, profil, search/filter (bot, status blokir,
  label, **tag, segmen**), notes, label, **tag berwarna, timeline, catatan
  (tambah/hapus)**, block/unblock, export CSV (**termasuk kolom tag**).
- **Segmen** — kelompok pengguna berdasarkan kriteria (tag, order,
  aktivitas) dengan preview jumlah anggota, daftar anggota, dan follow-up
  broadcast ke seluruh anggota segmen.
- **Groups** — info grup, pengaturan welcome (template `{{first_name}}`
  `{{chat_title}}`)/anti-spam/flood/block links/kata terlarang/aksi
  pelanggaran + durasi mute, **aksi moderasi manual (mute/ban/unmute/unban),
  kartu statistik, pengirim teratas,** moderation log. Aksi moderasi **hanya** berjalan bila
  bot adalah admin grup (dicek via `getChatMember`); bila bukan admin,
  aplikasi menampilkan peringatan dan tidak mengeksekusi moderasi.
- **Scheduler** — one-time, harian, mingguan, interval, cron
  (`"MENIT JAM * * DOW"`), timezone, pause/resume, history, retry gagal
  (maks 3x lalu dinonaktifkan + notifikasi).
- **Analytics** — pesan per hari (inbound/outbound), pengguna aktif/baru,
  performa automation, failed requests, command usage, filter tanggal, export
  CSV/JSON, cetak/PDF via print browser, **statistik per bot + export CSV**.
- **Integrations** — **MCP server** (`POST /mcp`, JSON-RPC 2.0, 7 tools,
  konfigurasi Claude Desktop) dan **outgoing webhooks** (event, HMAC-SHA256,
  log pengiriman, retry, kirim uji).
- **Logs** — log server live (200 baris terakhir) via WebSocket + fallback
  polling.
- **Notifications** — bot disconnect, error API, automation gagal, task
  selesai/gagal, aktivitas tidak biasa; event penting memicu notifikasi lokal
  Android.
- **Security** — token bot dienkripsi AES-256-GCM, token tidak pernah
  ditampilkan lengkap (hanya 4 karakter terakhir), PIN app lock (hash SHA-256
  + salt; kunci level aplikasi, bukan hardware-backed), API key, **personal
  API tokens** (tampil penuh hanya sekali saat dibuat, disimpan sebagai hash),
  rate limit, redaksi secret di log, session management, auto-lock. **Biometrik tidak ada
  di v1.1** (hanya PIN).
- **Backup & Restore** — backup SQLite terenkripsi AES-256-GCM dengan
  passphrase, validasi sebelum restore (dekripsi + header SQLite +
  `integrity_check`), history backup.

## Struktur proyek

```
TeleCore/
├── backend/            Rust (Axum, Tokio, SQLx, SQLite)
│   ├── src/            main, config, api (MCP, logs, webhooks, tokens,
│   │                   broadcast, search, per-bot analytics), bot, automation,
│   │                   scheduler, security, websocket, services, logging
│   └── migrations/     15 tabel v1 + tabel v1.1 (outgoing_webhooks,
│                       webhook_deliveries, api_tokens) + FK + index
├── frontend/           React + TypeScript + Vite + Tailwind v3
│   └── src/            17 halaman (termasuk Segments, Integrations + Logs), api client,
│                       stores (Zustand), hooks (WebSocket, notifikasi lokal),
│                       komponen, utils (statusBar, copy)
├── android/            Proyek Capacitor Android (applicationId com.telecore.app)
├── .github/workflows/  android-build.yml (CI: backend + frontend + APK debug)
├── docs/               ANDROID.md, SIGNING.md, MCP.md, CRM.md, GROUPS.md
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
Android SDK/JDK di lingkungan ini). Repo: `fazaroot/app-telecore`
(public, branch `main`).

1. Push ke branch `main` (atau jalankan workflow manual).
2. Workflow `.github/workflows/android-build.yml` berjalan otomatis: job
   `backend` (cargo test + build release, artifact binary server), job
   `frontend` (vitest + build), job `android` (build frontend, `cap sync`,
   Gradle `assembleDebug`, validasi APK > 1 MB).
3. Unduh **`telecore-debug.apk`** dari Artifacts di halaman Actions run,
   lalu instal di HP (izinkan "instal dari sumber tidak dikenal" bila diminta).
4. Di aplikasi, buka **Settings** → isi Backend URL (IP LAN/VPS tempat backend
   berjalan, mis. `http://192.168.1.10:8080`) + API key yang sama dengan
   `TELECORE_API_KEY` di server → Tes Koneksi.

> **v1.0.0:** CI run #45532748058 sukses percobaan pertama (~4 menit),
> APK 4,3 MB terverifikasi. v1.1.0 menunggu CI run setelah push ini.
> Release APK (signed) belum tersedia — panduan signing ada di
> [docs/SIGNING.md](docs/SIGNING.md).

Detail Android (permission, cleartext HTTP lokal, SDK 35): [docs/ANDROID.md](docs/ANDROID.md).

## Hasil verifikasi (v1.1, 2026-11-25, lingkungan Ubuntu 24.04)

Angka-angka ini adalah hasil pengecekan nyata, bukan klaim:

| Pemeriksaan | Hasil |
|---|---|
| `cargo test` (backend) | **59 passed, 0 failed** (41 dari v1 + 18 baru: dispatch MCP, HMAC webhook, hash token, render template, format log, validasi event) |
| `cargo build` (backend) | Lolos (binary `telecore-backend` v1.1.0) |
| `npx tsc --noEmit` (frontend) | **0 error** |
| `npm test` / vitest (frontend) | **15 passed, 0 failed** (11 dari v1 + 4 baru: parser baris log backend) |
| `npm run build` (frontend) | Lolos (`dist/` ~910 KB) |
| `npx cap sync android` | Lolos, 8 plugin Capacitor terdeteksi (termasuk StatusBar 7.0.6) |
| Smoke test API v1.1 (25 cek, server sementara + DB sementara) | **Semua lolos** — MCP initialize/ping/tools/list (7 tools)/tools/call/unknown tool -32602/unknown method -32601/tanpa auth 401; token personal dibuat → dipakai (200) → di-revoke → 401; preview template; search (kosong → 400); webhook dibuat (secret sekali) → test ke URL mati tercatat failed → retry attempts 1→2; broadcast ke 2 penerima (token palsu → 401 Telegram per penerima, job selesai `done`); logs mengembalikan baris nyata; per-bot + export CSV; endpoint v1 (bots, messages) tetap 200 |
| Verifikasi v1 (2026-11-25) | cargo test 41/41, vitest 8/8, smoke API 23/23, alur webhook end-to-end dengan update palsu |

Perbaikan integrasi yang dilakukan saat verifikasi v1.1 (ditemukan karena
frontend dan backend dibangun terpisah lalu dicocokkan): `GET /api/logs`
mengembalikan `{lines: string[]}` sedangkan frontend mencari key
`logs/items/data` — diperbaiki dengan parser baris log + normalisasi entri
(`frontend/src/api/logs.ts`, beserta 4 unit test baru); field delivery
webhook backend (`http_status`, `last_error`) dipetakan ke UI
(`http_status ?? response_status ?? status_code`,
`last_error ?? error`); placeholder `?` literal di `QueryBuilder` search
yang menyebabkan 500 — diperbaiki backend.

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

## Hasil verifikasi frontend v1.3.0 (2026-11-25)

| Pemeriksaan | Hasil |
|---|---|
| `npx tsc --noEmit` (frontend) | **0 error** |
| `npm test` / vitest (frontend) | **49 passed, 0 failed** (43 dari v1–v1.2 + 6 baru v1.3: regresi modal-tertutup Users/Groups/Segments, preview segmen, `criteriaSummary`) |
| `npm run build` (frontend) | Lolos |
| `npx cap sync android` | Lolos, 8 plugin Capacitor terdeteksi |

Catatan: angka di atas hanya frontend. Verifikasi backend v1.3 (cargo test,
smoke API endpoint baru) adalah tanggung jawab pekerjaan backend dan belum
termasuk di sini; endpoint v1.3 belum diuji live terhadap backend nyata.

## Batasan yang diketahui (v1.1)

- **Perbaikan header belum diuji di perangkat** — terverifikasi di build
  (`cap sync` + `npm run build`), bukan di POCO M6. Astr perlu memasang APK
  v1.1.0 dan memastikan tombol hamburger bisa diklik tanpa tertutup status bar.
- **Belum diuji dengan bot Telegram asli** — tidak ada token bot pengujian.
  Semua panggilan Telegram terverifikasi sampai ke api.telegram.org dengan
  token palsu (401), dan alur webhook/automation terverifikasi dengan update
  palsu. Pengujian live dengan bot test adalah langkah Astr di perangkat.
- Pemicu outgoing webhook dari update Telegram nyata (`message.inbound`,
  `automation.executed`, `bot.disconnected`, `scheduled_task.completed`)
  terpasang di titik yang benar dan non-blocking, tetapi **belum teruji
  end-to-end** dengan bot asli; hanya jalur test/retry yang terverifikasi.
- Event WS `"log"` belum diverifikasi dengan klien WebSocket live (ring
  buffer + `GET /api/logs` sudah terverifikasi).
- Status broadcast hanya in-memory (hilang saat server restart); maks 1000
  penerima per broadcast.
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
