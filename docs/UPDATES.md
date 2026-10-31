# Otomatik güncelleme

Kurulu KONİ (NSIS) kendini günceller: [tauri-plugin-updater](https://v2.tauri.app/plugin/updater/), imzalı kurulum, public dağıtım reposu.

## Akış
> Yedek yol: Actions çalışmazsa `client/scripts/release.ps1` aynı adımları yerelde yapar (imza anahtarı gerekir).

1. `client/src-tauri/tauri.conf.json` `version` artırılır, `docs/releases/v<sürüm>.md` yazılır, main'e girer.
2. `release` iş akışı (`.github/workflows/release.yml`):
   - `windows` işi kurulumu `TAURI_SIGNING_PRIVATE_KEY` ile imzalar (`KONI_<sürüm>_x64-setup.exe.sig`, `client/src-tauri/updater.conf.json`). İmza yoksa iş başarısız olur.
   - Kurulum, taşınabilir zip ve notlar bu (private) reponun release'ine konur; ayrıca `latest.json` hazırlanır.
   - `public` işi kurulum + zip + `latest.json` + `notes.md`'yi **`Savas07/koni-releases`** (public) reposunun `incoming` dalına tek commit olarak iter (her sürümde üzerine yazılır). Yalnızca o repoya yazabilen deploy key kullanılır (secret `KONI_RELEASES_DEPLOY_KEY`).
   - O reponun `publish` iş akışı `incoming`'i `v<sürüm>` Release'i yapar (`--latest`).
3. Uygulama `https://github.com/Savas07/koni-releases/releases/latest/download/latest.json` adresine bakar (`plugins.updater.endpoints`). Repo private olduğu için dosyalar ayrı public repoda; kaynak kod orada yok.

## İstemci
- `client/src-tauri/src/update.rs`: `update_check` (sürüm, not, `portable`), `update_install` (indir, imzayı doğrula, kur; ilerleme `update://progress`). Geliştirme derlemesi hiç bakmaz.
- Açılışta yeni sürüm varsa (henüz odada değilken) hemen kurulur: NSIS `passive` (`/P /R /UPDATE`) çalışır, uygulama kapanır, kurulum bitince yeniden açılır (~10 sn). Uygulama açıkken 6 saatte bir bakılır; bulunursa sağ panelde "Güncelle" kartı çıkar (oda bölünmez).
- Taşınabilir kopya (yanında `uninstall.exe` yok) kendini güncellemez, yalnızca etkinlikte "çıktı" der.
- İmza tutmazsa kurulum yapılmaz ("signature verification failed"), uygulama çalışmaya devam eder.

## İmza anahtarı
- Açık anahtar: `tauri.conf.json` → `plugins.updater.pubkey`.
- Özel anahtar ve parolası: secret `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; yedeği anahtarı üreten bilgisayarda `%USERPROFILE%\.koni-updater\` (`koni-updater.key`, `password.txt`). Güvenli bir yere yedekleyin.
- Anahtar kaybolursa: `npm run tauri signer generate` ile yenisi üretilir, `pubkey` ve secret'lar değiştirilir. Eski anahtarla kurulu sürümler yeni imzayı kabul etmez; o kullanıcılar bir kez elle kurar.

## Yerel imzalı kurulum
`$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw <anahtar>; $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = …` sonra `client/scripts/build-installer.ps1`. Değişkenler yoksa imzasız kurulum üretilir (güncelleme dosyası olmadan).
