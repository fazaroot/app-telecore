# Signing Release APK TeleCore

Panduan ini menjelaskan cara mengaktifkan signing release agar CI bisa
menghasilkan APK/AAB release yang ditandatangani kunci milik sendiri.
**Panduan saja — tidak ada keystore yang dibuat di repo ini.** Jangan pernah
me-commit keystore (`.jks`/`.keystore`) atau password-nya ke Git.

## 1. Buat keystore (sekali saja, di mesin tepercaya)

Jalankan di mesin yang punya JDK (bukan di repo, bukan di CI):

```bash
keytool -genkeypair -v \
  -keystore telecore-release.keystore \
  -alias telecore \
  -keyalg RSA -keysize 2048 -validity 10000
```

`keytool` akan meminta:

- **keystore password** — simpan baik-baik.
- **key password** — boleh sama dengan keystore password.
- Data pemilik (nama, organisasi, kota, negara) — isi sewajarnya.

Simpan file `telecore-release.keystore` di tempat aman (password manager /
media terenkripsi). **Kehilangan keystore = tidak bisa lagi memperbarui
aplikasi yang sama di Play Store.**

## 2. Siapkan secrets GitHub

Di repo GitHub → **Settings → Secrets and variables → Actions**, tambahkan:

| Secret | Isi |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | Keystore dalam bentuk base64 (lihat di bawah) |
| `ANDROID_KEYSTORE_PASSWORD` | Keystore password |
| `ANDROID_KEY_ALIAS` | `telecore` (atau alias yang dipakai saat membuat) |
| `ANDROID_KEY_PASSWORD` | Key password |

Encode keystore ke base64 (satu baris, tanpa wrap):

```bash
# Linux
base64 -w0 telecore-release.keystore

# macOS
base64 -i telecore-release.keystore
```

Salin output base64 ke secret `ANDROID_KEYSTORE_BASE64`.

## 3. Aktifkan signing config di Gradle

Tambahkan signing config ke `android/app/build.gradle` (belum ada di v1):

```groovy
android {
    signingConfigs {
        release {
            storeFile file(System.getenv("KEYSTORE_PATH") ?: "telecore-release.keystore")
            storePassword System.getenv("ANDROID_KEYSTORE_PASSWORD")
            keyAlias System.getenv("ANDROID_KEY_ALIAS")
            keyPassword System.getenv("ANDROID_KEY_PASSWORD")
        }
    }
    buildTypes {
        release {
            signingConfig signingConfigs.release
            minifyEnabled false
            proguardFiles getDefaultProguardFile('proguard-android.txt'), 'proguard-rules.pro'
        }
    }
}
```

Alternatif yang umum dipakai: baca dari `gradle.properties` lokal
(`~/.gradle/gradle.properties`, **bukan** yang di repo):

```properties
TELECORE_STORE_FILE=/path/ke/telecore-release.keystore
TELECORE_STORE_PASSWORD=...
TELECORE_KEY_ALIAS=telecore
TELECORE_KEY_PASSWORD=...
```

lalu di `build.gradle`:

```groovy
signingConfigs {
    release {
        storeFile file(TELECORE_STORE_FILE)
        storePassword TELECORE_STORE_PASSWORD
        keyAlias TELECORE_KEY_ALIAS
        keyPassword TELECORE_KEY_PASSWORD
    }
}
```

## 4. Tambahkan job release di CI

Contoh langkah tambahan di `.github/workflows/android-build.yml`
(job `android`, setelah langkah build debug — atau job `release` terpisah
dengan `needs: frontend` dan trigger tag `v*`):

```yaml
- name: Decode keystore
  run: |
    echo "${{ secrets.ANDROID_KEYSTORE_BASE64 }}" | base64 -d > android/telecore-release.keystore

- name: Build release APK
  working-directory: android
  env:
    KEYSTORE_PATH: telecore-release.keystore
    ANDROID_KEYSTORE_PASSWORD: ${{ secrets.ANDROID_KEYSTORE_PASSWORD }}
    ANDROID_KEY_ALIAS: ${{ secrets.ANDROID_KEY_ALIAS }}
    ANDROID_KEY_PASSWORD: ${{ secrets.ANDROID_KEY_PASSWORD }}
  run: ./gradlew assembleRelease --no-daemon
```

Untuk Play Store, ganti `assembleRelease` dengan `bundleRelease` agar
menghasilkan AAB (`android/app/build/outputs/bundle/release/app-release.aab`).

## 5. Periksa sebelum rilis

- [ ] `versionCode` di `android/app/build.gradle` dinaikkan setiap rilis
      (Play Store menolak versionCode yang sama).
- [ ] `versionName` diperbarui (mis. `1.0.1`).
- [ ] Cleartext HTTP (`android:usesCleartextTraffic`) sudah dihapus bila
      backend produksi sudah HTTPS (lihat `docs/ANDROID.md`).
- [ ] APK/AAB release diuji instal di perangkat sebelum dipublikasikan.
- [ ] Keystore dan password tersimpan di luar repo, dengan cadangan.
