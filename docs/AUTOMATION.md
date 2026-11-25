# Automation — Visual Builder (v1.4.0)

Automation TeleCore memakai **Visual Automation Builder**: workflow disusun
sebagai node **Trigger → Condition → Action** di kanvas vertikal dengan garis
penghubung. Klik node untuk mengeditnya di panel **Properties**. Action bisa
ditambah, dihapus, dan diurutkan naik/turun. Builder ini adalah builder node
visual dengan panel properties — bukan drag-drop.

Semua fitur automation gratis, tanpa paywall.

## Node

### Trigger

- `message_received` — pesan masuk.
- `keyword` — pesan mengandung kata kunci.
- `command` — pesan adalah command bot.
- `new_user`, `user_joined_group`, `scheduled` — trigger lain yang didukung
  backend.

### Condition

Condition memeriksa konteks pesan (mis. `text`, `chat_type`, `chat_id`,
`user_id`, `username`, `first_name`, `message_id`) dengan operator backend
(mis. teks mengandung kata kunci). Beberapa condition digabung dengan logika
AND/OR. Condition kata kunci tidak boleh kosong.

### Action

- **Kirim balasan** (`send_message`) — teks balasan mendukung template
  variables `{{first_name}}`, `{{username}}`, `{{chat_title}}`; chat tujuan
  opsional (default chat pemicu).
- **Simpan aktivitas** (`save_activity`) — menulis activity log untuk
  user/chat terkait. `text` wajib; `label` opsional.
- **Kirim notifikasi admin** (`notify_admin`) — membuat notifikasi in-app.
  Bila `admin_chat_id` diisi (ID chat numerik), pesan Telegram juga dikirim
  ke chat itu memakai bot workflow.
- Aksi lama tetap didukung: `send_template`, `add_label`, `save_note`,
  `delay` (termasuk `delay_seconds` per aksi, maks 30 detik).

## Contoh workflow

1. Trigger: pesan masuk.
2. Condition: teks mengandung kata kunci, mis. `harga`.
3. Action: kirim balasan berisi daftar harga.
4. Action: simpan aktivitas "pengguna menanyakan harga".
5. Action: kirim notifikasi admin.

## Validasi

Sebelum disimpan, builder dan API menolak workflow bila:

- bot belum dipilih atau nama workflow kosong;
- tidak ada action;
- nilai condition kosong;
- teks balasan/aktivitas/notifikasi kosong untuk aksi yang dipilih;
- tipe action tidak dikenal backend;
- `admin_chat_id` diisi tetapi bukan ID numerik valid.

Workflow lama tetap bisa dibuka di builder; aksi dan kondisi lama dimuat
tanpa dihilangkan. Bila workflow lama berisi tipe action yang tidak dikenal
backend, builder memintamu mengganti tipe itu sebelum menyimpan.
