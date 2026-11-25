# TeleCore v1.1 — MCP Server, Outgoing Webhooks & Personal API Tokens

Dokumen ini menjelaskan fitur backend v1.1. Semua endpoint di bawah
membutuhkan auth kecuali disebutkan lain.

## Auth

Semua endpoint `/api/*` dan `POST /mcp` memakai header `X-API-Key` dengan
salah satu dari:

1. **API key env** — nilai `TELECORE_API_KEY` server.
2. **Personal API token** — token `tc_...` yang dibuat lewat
   `POST /api/security/tokens` (lihat di bawah).

Auth gagal dijawab **HTTP 401** (bukan error JSON-RPC). Ini berlaku juga
untuk `/mcp` — konsisten di semua endpoint.

---

## 1. MCP Server — `POST /mcp`

JSON-RPC 2.0 Streamable HTTP di root (bukan `/api/mcp`). Satu request
JSON-RPC per HTTP request; batch request tidak didukung (error `-32600`).

### initialize

```bash
curl -X POST http://localhost:8080/mcp \
  -H "X-API-Key: <key>" -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}'
```

Respons:

```json
{"jsonrpc":"2.0","id":1,"result":{
  "protocolVersion":"2024-11-05",
  "capabilities":{"tools":{}},
  "serverInfo":{"name":"telecore-mcp","version":"1.1.0"}
}}
```

### ping

`{"jsonrpc":"2.0","id":2,"method":"ping"}` → `{"jsonrpc":"2.0","id":2,"result":{}}`

### tools/list

`{"jsonrpc":"2.0","id":3,"method":"tools/list"}` → daftar 7 tools:

| Tool | Deskripsi | Argumen |
|---|---|---|
| `list_bots` | Daftar bot | `status?` |
| `get_bot` | Detail satu bot | `bot_id` (wajib) |
| `send_message` | Kirim pesan teks via bot (mendukung template variables `{{first_name}}`, `{{username}}`, `{{chat_title}}`) | `bot_id`, `chat_id` (angka / string angka), `text`, `parse_mode?` |
| `list_users` | Daftar pengguna bot | `bot_id?`, `q?`, `limit?` (default 20, maks 100) |
| `get_analytics_summary` | Ringkasan analytics (sama dengan `GET /api/analytics/summary`) | `bot_id?`, `from?`, `to?` (RFC3339) |
| `list_workflows` | Daftar automation workflow | `bot_id?` |
| `list_scheduled_tasks` | Daftar scheduled task | `bot_id?` |

### tools/call

```bash
curl -X POST http://localhost:8080/mcp \
  -H "X-API-Key: <key>" -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":4,"method":"tools/call",
       "params":{"name":"list_bots","arguments":{}}}'
```

Respons sukses membawa `result.content[0].text` (JSON string) dan
`result.structuredContent` (objek yang sama).

### Error JSON-RPC

| Kode | Arti |
|---|---|
| `-32700` | Body bukan JSON valid |
| `-32600` | Bukan JSON-RPC 2.0 / batch request |
| `-32601` | Method tak dikenal |
| `-32602` | Tool tak dikenal atau parameter tidak valid |

### Contoh konfigurasi Claude Desktop

```json
{
  "mcpServers": {
    "telecore": {
      "url": "https://<host-backend-anda>/mcp",
      "headers": { "X-API-Key": "<personal-token-atau-api-key>" }
    }
  }
}
```

> Catatan: Claude Desktop versi yang mendukung remote MCP server memakai
> transport `url` seperti di atas. Bila klien MCP Anda hanya mendukung
> stdio, jalankan proxy HTTP→stdio sendiri — TeleCore tidak menyediakan
> transport stdio. URL backend harus dapat dijangkau dari mesin yang
> menjalankan Claude Desktop (mis. URL publik instabox).

---

## 2. Outgoing Webhooks

Event yang didukung: `message.inbound`, `automation.executed`,
`bot.disconnected`, `scheduled_task.completed`.

### CRUD

- `GET /api/outgoing-webhooks` — list (tanpa secret, hanya `has_secret`).
- `POST /api/outgoing-webhooks` — body:
  ```json
  {"name":"Notifikasi","url":"https://example.com/hook",
   "events":["message.inbound"],"secret":"opsional — bila kosong digenerate"}
  ```
  Respons create menampilkan `secret` **sekali saja** — simpan segera.
- `GET /api/outgoing-webhooks/:id`, `PUT /api/outgoing-webhooks/:id`
  (isi `secret` untuk mengganti; respons menampilkan secret baru sekali),
  `DELETE /api/outgoing-webhooks/:id`.
- `GET /api/outgoing-webhooks/:id/deliveries` — riwayat pengiriman.
- `POST /api/outgoing-webhooks/:id/test` — kirim event `test` sungguhan ke
  URL target, catat delivery.
- `POST /api/outgoing-webhooks/:id/deliveries/:delivery_id/retry` —
  kirim ulang payload tersimpan.

### Format pengiriman

`POST` JSON ke URL target (timeout 10 detik):

```json
{"event":"message.inbound","bot_id":"<id-bot>",
 "data":{"chat_id":123,"text":"halo","message_type":"text"},
 "sent_at":"2026-11-25T10:00:00Z"}
```

Header:

- `X-TeleCore-Event: <event>`
- `X-TeleCore-Signature: <HMAC-SHA256 hex dari body mentah, memakai secret webhook>`
  (hanya bila webhook punya secret; nama header dikirim lowercase oleh
  HTTP client — nama header HTTP tidak case-sensitive).

Verifikasi signature (penerima, contoh Python):

```python
import hmac, hashlib
expected = hmac.new(secret.encode(), raw_body, hashlib.sha256).hexdigest()
assert hmac.compare_digest(expected, request.headers["X-TeleCore-Signature"])
```

Secret **tidak pernah disimpan mentah**: disimpan terenkripsi AES-256-GCM
(HMAC butuh secret mentah saat pengiriman, jadi hash saja tidak cukup).
Pengiriman tidak pernah memblokir alur utama — tiap webhook dikirim lewat
task background; kegagalan hanya dicatat sebagai delivery `failed`.

Pemicu nyata: pesan masuk (`update_processor`, setelah pesan tersimpan),
automation dieksekusi (`automation` engine, setelah eksekusi dicatat —
termasuk status `blocked` oleh loop guard), bot disconnect
(`POST /api/bots/:id/disconnect`), scheduled task selesai (`scheduler` loop).

---

## 3. Personal API Tokens

- `GET /api/security/tokens` — list token (id, name, prefix,
  `created_at`, `last_used_at`). Hash dan token penuh **tidak pernah**
  dikembalikan.
- `POST /api/security/tokens` — body `{"name":"Claude Desktop"}`.
  Respons menampilkan token penuh **sekali saja**:
  ```json
  {"id":"...","name":"Claude Desktop","prefix":"tc_ab12cd34",
   "token":"tc_<32 hex>","created_at":"..."}
  ```
- `DELETE /api/security/tokens/:id` — cabut token (langsung tidak berlaku).

Format token: `tc_` + 32 karakter hex acak. Yang disimpan hanya SHA-256
hex dari token penuh + prefix (8 karakter pertama). Token dipakai sebagai
header `X-API-Key` — pengganti API key env — untuk semua endpoint `/api/*`
dan `/mcp`. `last_used_at` diupdate setiap kali token berhasil dipakai.
Membuat token pertama membutuhkan API key env (endpoint ini juga di
belakang auth).

---

## 4. Broadcast, Template Variables & Message Search

### Broadcast

- `POST /api/messages/broadcast`
  ```json
  {"bot_id":"<id>","text":"Halo {{first_name}}!","recipients":[{"chat_id":123},{"chat_id":456}]}
  ```
  → `{"broadcast_id":"...","total":2,"status":"running"}`. Pengiriman
  berjalan di background task, sequential dengan jeda 100ms antar chat.
  Tiap pesan disimpan sebagai pesan outbound (termasuk yang gagal).
- `GET /api/messages/broadcast/:id` — polling status:
  `{"broadcast_id","bot_id","total","sent","failed","status":"running|done",
    "results":[{"chat_id","status":"sent|failed","error"}]}`.

> **Batasan:** status broadcast hanya disimpan **in-memory** — hilang saat
> server restart. Setelah restart, `GET .../broadcast/:id` mengembalikan
> 404 untuk broadcast lama. Pesan yang sudah terkirim tetap tercatat di
> tabel `messages`.

### Template variables

`{{first_name}}`, `{{username}}`, `{{chat_title}}` diganti dari data
penerima di DB (`bot_users` / `bot_chats`) saat pengiriman via
`POST /api/messages` (atau `/messages/send`) dan broadcast. Variabel tak
dikenal dibiarkan apa adanya; nilai yang tidak diketahui diganti string
kosong.

- `POST /api/messages/preview`
  ```json
  {"text":"Halo {{first_name}} di {{chat_title}}","first_name":"Astr","chat_title":"Grup"}
  ```
  → `{"rendered":"Halo Astr di Grup"}`

### Message search

`GET /api/messages/search?q=halo&bot_id=<id>&limit=50` — cari di isi pesan
(LIKE, case-insensitive). `q` wajib tidak kosong (400 bila kosong).
Default `limit` 50, maks 200.

---

## 5. Live Logs

- `GET /api/logs?limit=200` — 200 baris log terakhir (format teks:
  `timestamp [LEVEL] pesan`), paling baru di akhir daftar. `limit`
  default 200, maks 200 (kapasitas ring buffer).
- Event log juga disiarkan ke klien WebSocket `/ws` sebagai event
  `{"type":"log","payload":{"line":"...","level":"INFO","message":"..."},"ts":"..."}`.
  Logging `fmt` yang sudah ada tetap berjalan — layer ini hanya menambah
  sink.

---

## 6. Per-Bot Analytics

- `GET /api/analytics/per-bot?from=&to=` →
  ```json
  [{"bot_id":"...","bot_name":"...","messages_in":10,"messages_out":8,
    "active_users":5,"new_users":2,"automations_run":3,"failed":1}]
  ```
  `failed` = pesan gagal + eksekusi automation gagal dalam rentang.
  `active_users` = distinct pengirim pesan inbound; `new_users` = baris
  `bot_users` yang dibuat dalam rentang.
- `GET /api/analytics/per-bot/export?from=&to=` — file CSV
  (`Content-Type: text/csv`, `Content-Disposition: attachment`,
  nama file `telecore-analytics-per-bot.csv`).

---

## 7. Versi

`GET /api/health` mengembalikan `"version":"1.1.0"` (dari
`CARGO_PKG_VERSION`, `Cargo.toml` v1.1.0). MCP `initialize` juga
mengembalikan `serverInfo.version: "1.1.0"`.
