# CRM — Tag, Segmen, Timeline (v1.3.0)

Semua fitur CRM di TeleCore **gratis, tanpa paywall di mana pun**.

## Tag

Tag adalah label berwarna yang ditempel ke pengguna, dipakai untuk
pengelompokan cepat dan sebagai kriteria segmen.

- **Lokasi:** halaman **Users** — badge warna tag tampil di tiap kartu user;
  kelola tag dari detail pengguna (modal).
- **Buat tag:** isi nama + pilih warna, tombol "Buat Tag".
- **Hapus tag:** ikon tempat sampah di samping nama tag pada daftar tag.
  Menghapus tag melepasnya dari semua pengguna.
- **Pasang/lepas tag:** dropdown "+ Pasang tag…" di detail pengguna;
  tombol × pada badge tag untuk melepas.
- **Filter:** dropdown "Semua tag" di atas daftar Users memfilter daftar
  berdasarkan tag (`GET /api/users?tag_id=...`).

### Endpoint

| Metode | Endpoint | Keterangan |
|---|---|---|
| GET | `/api/tags` | Daftar tag |
| POST | `/api/tags` | Buat tag `{name, color}` |
| PUT | `/api/tags/{id}` | Ubah tag |
| DELETE | `/api/tags/{id}` | Hapus tag |
| GET | `/api/users/{id}/tags` | Tag milik pengguna |
| POST | `/api/users/{id}/tags` | Pasang tag `{tag_id}` |
| DELETE | `/api/users/{id}/tags/{tag_id}` | Lepas tag |

Tag: `{id, name, color, created_at}`.

## Segmen

Segmen adalah kelompok pengguna yang didefinisikan oleh kriteria — anggota
dihitung ulang oleh server dari data terbaru, bukan daftar statis.

- **Lokasi:** halaman baru **Segmen** (drawer, rute `/segments`).
- **Daftar segmen:** nama, ringkasan kriteria, dan jumlah anggota.
- **Buat/edit segmen:** tombol "+ Buat Segmen" / "Edit" membuka form
  pembangun kriteria.
- **Klik segmen:** membuka daftar anggota + composer follow-up.

### Pembangun kriteria

Semua kriteria bersifat opsional dan digabung dengan AND:

| Kriteria | Arti |
|---|---|
| Tag (pilih satu atau lebih) | Pengguna punya **salah satu** tag yang dipilih (OR di dalam tag) |
| Sudah pernah order | Pengguna pernah melakukan order |
| Aktif ≤ N hari terakhir | Terakhir aktif dalam N hari terakhir |
| Tidak aktif > N hari | Terakhir aktif lebih dari N hari lalu |
| Minimal jumlah pesan | Jumlah pesan pengguna ≥ N |

### Preview jumlah anggota

Setiap kali kriteria berubah, aplikasi memanggil
`POST /api/segments/preview` (debounce 400 ms) dan menampilkan
"Preview: N anggota cocok dengan kriteria ini" **sebelum** segmen disimpan —
jadi tidak perlu menyimpan dulu untuk tahu hasilnya.

### Follow-up ke segmen

Di modal anggota segmen, tulis pesan lalu "Kirim ke Semua Anggota".
Server mengirim ke semua anggota dan mengembalikan hasil
`{total, sent, failed, broadcast_id?}` yang langsung ditampilkan
("Terkirim: X · Gagal: Y · Total: Z"). Pesan memakai template variable yang
sama seperti broadcast (`{{first_name}}`, `{{username}}`, `{{chat_title}}`)
bila didukung server.

### Filter pengguna berdasarkan segmen

Dropdown "Semua segmen" di halaman Users memfilter daftar
(`GET /api/users?segment_id=...`).

### Endpoint

| Metode | Endpoint | Keterangan |
|---|---|---|
| GET | `/api/segments` | Daftar segmen |
| POST | `/api/segments` | Buat segmen `{name, criteria}` |
| GET | `/api/segments/{id}` | Detail segmen |
| PUT | `/api/segments/{id}` | Ubah segmen |
| DELETE | `/api/segments/{id}` | Hapus segmen (anggota tidak ikut terhapus) |
| POST | `/api/segments/preview` | Preview `{criteria}` → `{count}` |
| GET | `/api/segments/{id}/members` | Anggota (paginasi `{items, total, page, per_page}`) |
| POST | `/api/segments/{id}/follow-up` | Kirim pesan `{message, parse_mode?}` → `{total, sent, failed, broadcast_id?}` |

Kriteria JSON:

```json
{
  "any_tags": ["<tag-id>"],
  "has_ordered": true,
  "last_active_within_days": 7,
  "last_active_over_days_ago": 30,
  "min_messages": 10
}
```

## Timeline pengguna

Detail pengguna menampilkan **Timeline** kronologis — gabungan aktivitas
pengguna:

| Jenis (`kind`) | Isi |
|---|---|
| `message` | Pesan masuk/keluar |
| `order` | Order (mis. pembayaran Stars) |
| `note` | Catatan yang ditambahkan |
| `tag` | Tag dipasang/dilepas |

Endpoint: `GET /api/users/{id}/timeline` →
`{items: [{kind, created_at, summary, data?}]}`.

## Catatan pengguna

- **Riwayat catatan:** daftar catatan di detail pengguna, masing-masing bisa
  dihapus (ikon tempat sampah).
- **Tambah catatan:** kolom input + tombol "Tambah" — catatan tersimpan
  terpisah per entri (`GET/POST /api/users/{id}/notes`,
  `DELETE /api/users/{id}/notes/{note_id}`).
- **Catatan profil (lama):** kolom "Catatan profil" + "Label" dari v1 tetap
  ada dan disimpan lewat `PUT /api/users/{id}` seperti sebelumnya.

## Export CSV

Tombol **Export CSV** di halaman Users tetap berfungsi seperti sebelumnya,
kini menyertakan **kolom tags** di file CSV (endpoint `GET /api/users/export`).

## Batasan yang diketahui

- Anggota segmen dihitung server dari kriteria saat diminta; bila data
  pengguna berubah (mis. tag dilepas), anggota segmen ikut berubah.
- Follow-up mengirim ke anggota segmen **saat itu**; pengguna yang masuk
  segmen setelah pengiriman tidak menerima pesan tersebut.
- Timeline hanya menampilkan aktivitas yang dicatat server; pesan yang
  dihapus di Telegram tetap tampil bila sudah tercatat.
