# TeleCore untuk Android

Dokumen ini menjelaskan cara membangun APK TeleCore, menginstalnya di
perangkat, dan menghubungkannya ke backend.

## Arsitektur v1 — baca dulu

APK TeleCore **hanya berisi frontend React** (dibungkus Capacitor) — backend
Rust **tidak tertanam di dalam APK**. Backend adalah server terpisah yang
harus berjalan di server/PC/Termux, dan aplikasi terhubung ke backend lewat
URL yang diatur di menu **Settings** di dalam aplikasi.

```
┌─────────────────────┐   HTTP(S) / WebSocket   ┌──────────────────────┐
│  TeleCore APK       │ ───────────────────────▶ │  telecore-backend    │
│  (Capacitor + React)│   URL diatur di Settings │  (Axum + SQLite)     │
└─────────────────────┘                          └──────────────────────┘
```

Menanam server Axum lengkap di dalam WebView Capacitor tidak dicoba di v1;
pendekatan ini didokumentasikan apa adanya, bukan disembunyikan.

## Membangun APK (GitHub Actions)

Tidak ada build Gradle lokal — APK hanya dibangun di GitHub Actions.

1. Push ke branch `main` (atau jalankan manual lewat tab **Actions** →
   **TeleCore CI** → **Run workflow**).
2. Workflow `.github/workflows/android-build.yml` menjalankan tiga job:
   - `backend` — `cargo test` + `cargo build --release`, mengunggah binary
     `telecore-backend` Linux x86_64.
   - `frontend` — `npm ci`, `npm test`, `npm run build` di `frontend/`.
   - `android` — `needs: frontend`; build ulang frontend, `npx cap sync
     android`, lalu `./gradlew assembleDebug`. APK divalidasi (harus ada dan
     berukuran > 1 MB) sebelum diunggah.
3. Unduh artifact **`telecore-debug-apk`** dari halaman run yang berhasil.
   Di dalamnya ada `app-debug.apk`.

> Catatan: workflow sengaja **tidak** memakai
> `android-actions/setup-android` — paket itu pernah gagal karena mencoba
> menginstal paket `tools` yang sudah usang. Runner `ubuntu-latest` sudah
> menyediakan Android SDK.

## Menginstal APK di perangkat

1. Di perangkat Android, izinkan instalasi dari sumber tak dikenal untuk
   browser/file manager yang dipakai (Pengaturan → Keamanan → Instal aplikasi
   tak dikenal).
2. Buka `app-debug.apk` yang diunduh dan instal.
3. Syarat minimum: **Android 8.0 (API 26)** ke atas.
4. APK ini adalah build **debug** (ditandatangani kunci debug otomatis) —
   cocok untuk penggunaan pribadi/pengujian, bukan untuk distribusi Play
   Store. Untuk build release yang ditandatangani, lihat
   [SIGNING.md](SIGNING.md).

## Menjalankan backend & menghubungkan aplikasi

1. Jalankan backend di mesin yang terjangkau dari HP:
   - **Di PC/server (Linux):** unduh artifact `telecore-server-linux-x86_64`
     dari CI yang sama, beri izin eksekusi, lalu jalankan:
     ```bash
     chmod +x telecore-backend
     ./telecore-backend
     ```
     Secara default backend mendengarkan di `http://0.0.0.0:3000`
     (periksa `.env.example` / README backend untuk variabel yang tersedia).
   - **Di Termux (HP yang sama):** jalankan binary yang sama dari Termux,
     lalu pakai URL `http://127.0.0.1:3000` di aplikasi.
   - **Dari source:** `cargo run --release --manifest-path backend/Cargo.toml`.
2. Pastikan HP dan backend berada di jaringan yang sama (Wi-Fi/LAN yang
   sama) bila backend berjalan di PC — pakai alamat IP LAN PC, misal
   `http://192.168.1.10:3000`, bukan `localhost` (localhost di HP berarti HP
   itu sendiri).
3. Buka aplikasi TeleCore → **Settings** → isi **Backend URL** dengan alamat
   backend, lalu simpan. Aplikasi memakai URL itu untuk semua panggilan
   REST dan WebSocket.

## Catatan keamanan: cleartext HTTP lokal

`AndroidManifest.xml` mengaktifkan `android:usesCleartextTraffic="true"`.
Ini **sengaja** agar v1 bisa terhubung ke backend lokal via `http://`
(IP LAN) saat pengembangan — tanpa ini Android memblokir semua traffic
HTTP non-TLS.

Risikonya: traffic HTTP bisa disadap/dimodifikasi di jaringan lokal.
Untuk penggunaan di luar jaringan rumah tepercaya:

- Jalankan backend di belakang HTTPS (reverse proxy Nginx/Caddy dengan
  sertifikat TLS), dan isi Backend URL dengan `https://...`.
- Setelah semua pemakaian beralih ke HTTPS, hapus atribut
  `android:usesCleartextTraffic="true"` dari
  `android/app/src/main/AndroidManifest.xml`.

## Ikon & splash screen

- Launcher icon dan splash screen dibuat khusus untuk TeleCore: pesawat
  kertas putih di atas biru `#526FE5` (semua densitas `mdpi`–`xxxhdpi`,
  termasuk foreground adaptive icon dan splash portrait/landscape).
- Splash screen dikonfigurasi di `capacitor.config.ts`:
  `launchShowDuration` 1500 ms, warna latar `#526FE5`.
- Status bar: gaya `light` dengan latar `#526FE5`.

## Detail build Android

| Item | Nilai |
| --- | --- |
| applicationId | `com.telecore.app` |
| minSdk | 26 (Android 8.0) |
| compileSdk / targetSdk | 35 |
| versionCode / versionName | 1 / `1.0.0` |
| Capacitor | 7.x (`@capacitor/core` 7.6.x) |
| Gradle wrapper | 8.11.1, AGP 8.7.2, JDK 21 (di CI) |

> **Mengapa compileSdk/targetSdk 35, bukan 34?** Capacitor 7 dan dependensi
> AndroidX-nya (mis. `androidx.core` 1.15.0) mewajibkan compileSdk API 35;
> menurunkan ke 34 akan menggagalkan build Gradle di CI. `minSdk` tetap 26
> sesuai target proyek.

## Izin (permissions)

`AndroidManifest.xml` mendeklarasikan:

- `INTERNET` — terhubung ke backend.
- `ACCESS_NETWORK_STATE` — deteksi status jaringan (plugin Network).
- `POST_NOTIFICATIONS` — notifikasi lokal (Android 13+ meminta izin runtime).
- `VIBRATE` — getar untuk notifikasi.
- `RECEIVE_BOOT_COMPLETED` — penjadwalan ulang notifikasi setelah reboot.

## Pengembangan lokal (tanpa Android Studio)

```bash
# 1. Build frontend
cd frontend && npm ci && npm run build && cd ..

# 2. Sinkronkan aset web ke proyek Android
npm run sync        # = npx cap sync android

# 3. APK dibangun di GitHub Actions (tidak ada JDK/SDK lokal)
```

Setiap perubahan frontend harus diikuti `npm run build` + `npm run sync`
sebelum APK dibangun ulang di CI. Workflow CI sudah melakukan kedua langkah
itu secara otomatis.

## Batasan v1

- Backend **tidak tertanam** di APK; aplikasi tidak berguna tanpa backend
  yang berjalan terpisah.
- Hanya tersedia APK **debug**; belum ada signing release
  (lihat [SIGNING.md](SIGNING.md)).
- Koneksi backend lokal memakai HTTP cleartext — lihat catatan keamanan
  di atas.
- Notifikasi memakai `LocalNotifications` (lokal, dijadwalkan dari
  aplikasi); belum ada push notification server (FCM).
