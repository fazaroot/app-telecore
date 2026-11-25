# TeleCore Backend

Server backend TeleCore (Telegram Bot Control Center) — Rust + Axum + SQLite.
REST API + WebSocket broadcast. Berjalan di server/PC/Termux, **bukan** di dalam
APK: APK hanya berisi frontend yang terhubung ke server ini lewat URL yang bisa
dikonfigurasi di Settings.

## Build & jalan

```bash
# Build (RAM kecil: batasi job paralel)
CARGO_BUILD_JOBS=1 cargo build

# Test
CARGO_BUILD_JOBS=1 cargo test

# Jalan (development)
TELECORE_API_KEY=isi-api-key-yang-kuat \
DATABASE_URL=sqlite:telecore.db \
cargo run
```

Server mendengarkan `TELECORE_HOST:TELECORE_PORT` (default `127.0.0.1:8080`).

## Environment

| Variabel | Default | Keterangan |
|---|---|---|
| `TELECORE_HOST` | `127.0.0.1` | Alamat bind |
| `TELECORE_PORT` | `8080` | Port |
| `DATABASE_URL` | `sqlite:telecore.db` | Path SQLite (`sqlite:path` atau `sqlite:/abs/path.db`) |
| `TELECORE_API_KEY` | _(kosong)_ | **Wajib.** Header `X-API-Key` untuk semua `/api/*` kecuali `/api/health` dan webhook Telegram. Bila kosong, semua endpoint API menolak request (401). |
| `TELECORE_ENCRYPTION_KEY` | _(generate otomatis)_ | Kunci AES-256-GCM untuk token bot: hex 64 karakter atau base64 44 karakter. Bila tidak diset, kunci digenerate sekali dan disimpan ke `telecore.key` (mode 0600). **Backup file/kunci ini** — tanpanya token terenkripsi tidak bisa didekripsi. |
| `RUST_LOG` | `info` | Level log |

Migrasi database dijalankan otomatis saat server start (`sqlx::migrate!`, 15 tabel).

## API ringkas

Envelope konsisten:

```json
{"success": true, "data": {...}}
{"success": false, "error": {"code": "VALIDATION_ERROR", "message": "..."}}
```

| Area | Endpoint utama |
|---|---|
| Health | `GET /api/health` (tanpa auth) |
| Bots | `GET/POST /api/bots`, `GET/PUT/DELETE /api/bots/{id}`, `POST /api/bots/{id}/connect`, `/disconnect`, `/test`, `GET/PUT /api/bots/{id}/config`, `POST /api/bots/{id}/webhook`, `GET /api/bots/export`, `POST /api/bots/import` |
| Pesan | `GET /api/messages`, `POST /api/messages` (kirim teks/media; `chat_id` boleh string angka, `message_type`/`keyboard`/`media.data_url` didukung), `POST /api/messages/send` (alias), `POST /api/messages/send-media` (multipart upload), `POST /api/messages/{id}/retry`, `GET /api/messages/history` |
| Command | `GET/POST /api/commands`, `GET/PUT/DELETE /api/commands/{id}`, `POST /api/commands/{id}/test` |
| User | `GET /api/users`, `GET/PUT /api/users/{id}`, `POST /api/users/{id}/block`, `/unblock`, `GET /api/users/export?format=csv` |
| Grup | `GET /api/groups`, `GET /api/groups/{id}`, `PUT /api/groups/{id}` (update settings, body `{settings}`), `PUT /api/groups/{id}/settings` (alias), `GET /api/groups/{id}/moderation-logs`, `POST /api/groups/{id}/moderate` |
| Automation | `GET/POST /api/automation`, `GET/PUT/DELETE /api/automation/{id}`, `POST /api/automation/{id}/toggle`, `GET /api/automation/executions`, `GET /api/automation/{id}/executions` |
| Scheduler | `GET/POST /api/scheduler`, `GET/PUT/DELETE /api/scheduler/{id}`, `POST /api/scheduler/{id}/pause`, `/resume`, `GET /api/scheduler/history`, `GET /api/scheduler/{id}/history` |
| Analytics | `GET /api/analytics/summary`, `GET /api/analytics/export?format=csv\|json` |
| Notifikasi | `GET /api/notifications`, `POST /api/notifications/{id}/read`, `POST /api/notifications/read-all`, `DELETE /api/notifications/{id}` |
| Keamanan | `GET /api/security/status`, `POST /api/security/change-pin`, `/lock`, `/unlock`, `GET /api/security/sessions` |
| Backup | `POST /api/backups`, `GET /api/backups`, `POST /api/backups/{id}/restore`, `DELETE /api/backups/{id}` |
| Settings | `GET/PUT /api/settings` |
| WebSocket | `GET /ws` — event `{"type": "activity"\|"bot_status"\|"message"\|"notification"\|"stats", "payload": {...}, "ts": "..."}` |
| Webhook TG | `POST /api/webhooks/telegram/{bot_id}` (tanpa auth API key) |

Rate limit: 120 request/menit per IP (in-memory).

## Pilihan desain penting

### Export token terenkripsi

`GET /api/bots/export` **tidak pernah** mengeluarkan token plaintext. Token
diekspor dalam bentuk terenkripsi (`token_encrypted`, AES-256-GCM) persis
seperti yang tersimpan di database. Saat import (`POST /api/bots/import`),
token terenkripsi dipakai langsung — dan ini **hanya berhasil bila server
tujuan memakai `TELECORE_ENCRYPTION_KEY` yang sama**. Bila kunci berbeda,
bot tersebut dilewati dengan pesan error yang jelas di hasil import
(`skipped` + `errors`). Alternatif: import juga menerima field `token`
plaintext (akan dienkripsi saat import), tetapi export tidak pernah
menghasilkannya.

### Polling vs webhook (mutual exclusion)

Long polling dan webhook **tidak boleh aktif bersamaan** untuk bot yang sama
(Telegram akan menolak update ganda). Aturannya:

- Mengaktifkan webhook (`POST /api/bots/{id}/webhook` action `set`, atau
  `PUT /api/bots/{id}/config` dengan `webhook_enabled: true`) menghentikan
  task polling bot itu dan memanggil `setWebhook`.
- Mengaktifkan polling (`polling_enabled: true`) menghapus webhook di Telegram
  (`deleteWebhook`, best-effort) lalu menjalankan task polling.
- Saat server start, task polling dijalankan untuk setiap bot aktif dengan
  `polling_enabled = true` dan `webhook_enabled = false`.

### Moderasi grup

Aksi moderasi (`/api/groups/{id}/moderate` dan moderasi otomatis dari pesan
masuk) hanya dieksekusi bila bot adalah admin/creator di grup — dicek langsung
via `getChatMember` ke Telegram. Bila bukan admin, API mengembalikan 403 yang
jelas, dan moderasi otomatis hanya mencatat log tanpa menghapus pesan.

### Block user

`block` adalah flag internal TeleCore. Telegram Bot API tidak menyediakan
"block user" — yang bisa memblokir adalah user terhadap bot. Flag ini dipakai
aplikasi untuk menandai user yang diabaikan.

### Restore backup

Restore memvalidasi dulu: dekripsi dengan passphrase → cek header SQLite →
`PRAGMA integrity_check` pada salinan sementara. Bila valid, file hasil restore
ditulis ke `<db>.restore-pending` dan diterapkan saat server restart berikutnya
(file DB yang sedang dipakai tidak boleh ditimpa langsung). Respons restore
menyatakan `requires_restart: true`.

### Rotasi API key

API key berasal dari env `TELECORE_API_KEY` dan sengaja **tidak bisa diubah
via API** (agar tidak bisa dicuri lewat endpoint). Rotasi: ubah env, restart
server.

## Batasan yang diketahui (v1)

- `avg_response_time_ms` di analytics bernilai `null` — waktu respons bot
  belum dilacak.
- Retry pesan hanya didukung untuk pesan teks (konten media tidak tersimpan
  sebagai file).
- Cron scheduler mendukung format `"MENIT JAM * * *"` dan `"MENIT JAM * * DOW"` (DOW 0–6, 0 = Minggu). Task `daily`/`weekly` yang menyertakan `cron_expression` dijadwalkan pada jam/hari cron tersebut (bukan +24 jam dari waktu pembuatan).
  (harian pada jam:menit di timezone task).
- WebSocket `/ws` tidak memakai auth API key (bukan bagian `/api/*`); jalankan
  server di jaringan tepercaya atau di belakang reverse proxy bila perlu.
