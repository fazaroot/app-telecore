# Manajemen Grup (v1.3.0)

Semua fitur manajemen grup di TeleCore **gratis, tanpa paywall di mana pun**.

## Membuka pengaturan grup

Halaman **Groups** → klik kartu grup → modal detail. Modal berisi, berurutan:
peringatan bila bot bukan admin, kartu statistik, pengaturan grup, aksi
moderasi manual, dan log moderasi.

> **Syarat admin:** aksi moderasi (hapus pesan, mute, ban) hanya berjalan
> bila bot adalah **admin** grup (dicek via `getChatMember`). Bila bot bukan
> admin, aplikasi menampilkan peringatan kuning dan aksi tidak dieksekusi
> sampai bot dijadikan admin.

## Pengaturan grup

Semua pengaturan disimpan per grup via `PUT /api/groups/{id}/settings`
(partial — hanya field yang dikirim yang diubah).

| Pengaturan | Arti |
|---|---|
| Pesan sambutan (welcome) on/off | Aktif/nonaktifkan sapaan anggota baru |
| Pesan sambutan | Teks sapaan. Template: `{{first_name}}` (nama anggota baru), `{{chat_title}}` (nama grup). Contoh: `Selamat datang {{first_name}} di {{chat_title}}!` |
| Anti-spam on/off | Aktif/nonaktifkan deteksi spam otomatis |
| Batas flood (pesan) | Jumlah pesan maksimum dalam satu window sebelum dianggap flood |
| Window flood (detik) | Durasi window flood dalam detik |
| Blokir tautan (block links) | Hapus/blokir pesan yang berisi tautan |
| Kata terlarang | Daftar kata yang memicu aksi pelanggaran; tambah lewat input + tombol "Tambah" (atau Enter), hapus lewat tombol × pada chip kata |
| Aksi pelanggaran | Yang dilakukan saat pelanggaran terdeteksi: `delete` (hapus pesan), `warn` (beri peringatan), atau `mute` (mute pengguna) |
| Durasi mute (menit) | Lama mute otomatis saat aksi pelanggaran = mute |

Endpoint:

| Metode | Endpoint | Keterangan |
|---|---|---|
| GET | `/api/groups/{id}/settings` | Ambil pengaturan |
| PUT | `/api/groups/{id}/settings` | Simpan pengaturan (partial) |

Respons pengaturan:

```json
{
  "chat_id": -1001234567890,
  "welcome_enabled": true,
  "welcome_message": "Selamat datang {{first_name}} di {{chat_title}}!",
  "anti_spam_enabled": true,
  "flood_limit": 5,
  "flood_window_seconds": 10,
  "block_links": false,
  "banned_words": ["spam", "judi"],
  "violation_action": "delete",
  "mute_duration_minutes": 60
}
```

Catatan kompatibilitas: pengaturan lama pra-v1.3 (`keyword_filter`,
`link_filter`, `flood_detection`, `warnings_enabled`, `max_warnings`) tetap
diterima UI bila server mengembalikannya (dipetakan ke field v1.3 yang
setara), tetapi penyimpanan memakai kontrak v1.3 di atas.

## Moderasi otomatis

Moderasi otomatis berjalan di server berdasarkan pengaturan di atas:

1. Pesan masuk diperiksa terhadap kata terlarang, tautan (bila block links
   aktif), dan laju flood (batas + window).
2. Bila melanggar, server menjalankan **aksi pelanggaran** yang dipilih
   (hapus / peringatan / mute selama durasi mute).
3. Setiap tindakan otomatis tercatat di log moderasi.

## Moderasi manual

Bagian **Moderasi Anggota** di modal detail grup:

1. Isi **User ID Telegram** (angka) anggota yang ditindak.
2. Pilih aksi: `mute`, `unmute`, `ban`, atau `unban`.
3. Untuk mute/ban, isi **durasi (menit)**.
4. Isi **alasan** (opsional, tercatat di log).
5. Tekan "Jalankan Aksi".

Endpoint: `POST /api/groups/{id}/moderate`
`{user_id, action: "mute"|"unmute"|"ban"|"unban", duration_minutes?, reason?}`.

## Statistik grup

Kartu statistik di bagian atas modal detail (diambil dari
`GET /api/groups/{id}/stats`):

| Kartu | Arti |
|---|---|
| Anggota | Jumlah anggota grup saat ini |
| Pesan hari ini | Pesan yang tercatat hari ini |
| Pesan 7 hari | Pesan yang tercatat 7 hari terakhir |
| User aktif 7 hari | Pengguna berbeda yang aktif 7 hari terakhir |
| Pengirim teratas (7 hari) | Daftar user dengan pesan terbanyak + jumlah pesannya |

Respons:

```json
{
  "member_count": 42,
  "messages_today": 120,
  "messages_7d": 800,
  "active_users_7d": 30,
  "top_senders": [{ "user_id": 123, "username": "budi", "first_name": "Budi", "count": 55 }]
}
```

## Log moderasi

Bagian **Log Moderasi** di bawah modal detail menampilkan riwayat tindakan
moderasi (otomatis maupun manual), terbaru di atas: aksi, alasan, user ID,
dan waktu. Endpoint: `GET /api/groups/{id}/moderation-log` (paginasi
`{items, total, page, per_page}` atau array langsung).

## Batasan yang diketahui

- Moderasi membutuhkan bot admin; tanpa itu semua aksi (otomatis maupun
  manual) tidak berjalan dan hanya tercatat sebagai peringatan di UI.
- Statistik dihitung dari pesan yang tercatat server; pesan sebelum bot
  ditambahkan ke grup tidak terhitung.
- Durasi mute/ban mengikuti batas Telegram; durasi sangat panjang dapat
  ditolak Telegram dan tercatat gagal di log.
