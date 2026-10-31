# KONİ istemcisi (Faz 0 kokpit spike)

Tauri v2 + Svelte 5. Video, ana pencerenin altındaki native bir child HWND'de libmpv tarafından çizilir (sol üst). Web UI yalnızca sağ kolon ve alt bardadır, video alanına hiçbir şey çizmez ([docs/design/client.md §3.1–3.3](../docs/design/client.md), [ADR 0001](../docs/adr/0001-kokpit-modeli.md)).

## Yapı
- `src/` Svelte kokpit UI. `src/lib/player.ts` IPC sarmalayıcıları.
- `src-tauri/` Tauri uygulaması (`koni`). IPC komutları `src/commands.rs`, oda oturumu `src/sync.rs`, medya açma/çözme `src/media.rs`, Sanal Yakalayıcı `src/sniffer.rs` + `src/sniffer.js`, "Herkes İndirsin" `src/download.rs`, hesap `src/account.rs`.
- `resolver/` URL sınıflandırma + yt-dlp ile stream çözme (`koni-resolver`).
- `crates/koni-player/` libmpv sarmalayıcı. DLL çalışma anında `libloading` ile yüklenir (derleme zamanı link yok, LGPL: DLL değiştirilebilir).
- `crates/koni-surface/` `VideoSurface` trait'i, saf `video_rect` hesabı, `Win32ChildSurface` (ana pencere subclass'ı, `WM_SIZE`/`WM_WINDOWPOSCHANGED`/`WM_DPICHANGED` → `SetWindowPos`).

## Windows'ta çalıştırma
1. Node 22, Rust stable, WebView2 (Windows 10/11'de hazır).
2. `libmpv-2.dll`'i `target/debug/` (veya `target/release/`) içine, exe'nin yanına koy. Geliştirme için herhangi bir build olur; dağıtımda LGPL build kullanılır (`docs/LIBMPV.md`).
3. `npm ci` sonra `npx tauri dev`.

Kısa yol: KONİ kuruluysa `powershell -ExecutionPolicy Bypass -File scripts/dev.ps1 -Pull`. Son kodu çeker, DLL ve yt-dlp'yi kurulu KONİ'den alır, uygulamayı açar. Arayüz değişiklikleri anında yenilenir; CI ya da sürüm beklemek gerekmez.

CI (`.github/workflows/client.yml`) her PR'da Windows'ta derler, libmpv smoke testini koşar ve `koni-win64-dev` artefaktını (exe + DLL) üretir.

## Sanal Yakalayıcı (yt-dlp'nin tanımadığı siteler)
"Aç" web adresini önce yerelde çözer (sonuç 60 sn önbellekte; oda medyası yüklenirken yt-dlp ikinci kez çalışmaz). yt-dlp `Unsupported URL` derse adres ana pencereye bağlı geçici bir pencerede (`sniffer`) açılır; kullanıcı sayfada videoyu oynatır.
- `sniffer.js` her çerçeveye belge oluşurken enjekte edilir (`initialization_script_for_all_frames`): `fetch`, `XMLHttpRequest.open`, `<video>` `loadstart` ve `PerformanceObserver` kaynakları izlenir. Yol `.m3u8`/`.mpd`/`.mp4` ya da yanıt türü HLS/DASH/`video/*` (TS/segment hariç) olan ilk istek `window.__TAURI__.core.invoke('stream_captured', {url, referer, title})` ile bildirilir; çapraz kökenli iframe `postMessage` ile üst çerçeveye iletir.
- Kullanıcı etkileşiminden önce görülen istekler (otomatik reklam, ön yükleme) hemen gönderilmez; kullanıcı bir videoyu oynatınca 1 sn içinde yeni aday gelmezse kullanılır (manifest mp4'ten öncelikli).
- Akış hemen alınmaz: sayfanın altında "KONİ: video bulundu · KONİ'de aç" çubuğu çıkar; kullanıcı sitede başka kaynak (dublaj/altyazılı) seçerse yeni manifest öncekinin yerine geçer. Düğmeye basınca Rust pencereyi kapatır; akış normal "Aç" yolundan gider: odada `cmd media` (`referer` ile, PROTOCOL p0.7), değilse doğrudan mpv. Yakalanan akış mpv'ye `referrer` + tarayıcı `user-agent` seçenekleriyle verilir.
- Pop-up / `target="_blank"` istekleri reddedilir (`on_new_window`).
- Güvenlik: yakalayıcıda rastgele site çalışır. `build.rs` uygulama ACL manifestini tanımlar, bu yüzden her komut izin ister: ana pencere hepsine `capabilities/default.json` ile, yakalayıcı (uzak köken) yalnızca `stream_captured`'a `capabilities/sniffer.json` ile erişir. Yeni komut `build.rs` listesine ve `default.json`'a (`allow-<komut-adı>`) eklenmeli, yoksa "not allowed by ACL" hatası verir.
- Sınırlar: LGPL FFmpeg'de DASH (`.mpd`) demuxer'ı yok (libxml2 gerekir), yakalanır ama mpv açamaz. Çerez/oturum gerektiren ya da IP'ye bağlı imzalı akışlar odadaki diğer üyelerde açılmayabilir (her üye yakalayanın adresini kullanır). Web Worker içinden yapılan istekler görülmez.

## Windows kurulum dosyası
```
powershell -ExecutionPolicy Bypass -File scripts/build-installer.ps1 -Libmpv <libmpv-lgpl-win64.zip> [-Portable]
```
`libmpv-lgpl-win64` artefaktı (docs/LIBMPV.md) `src-tauri/vendor/win64/`'e açılır (gitignore), en güncel `yt-dlp.exe` indirilir, `tauri build --config src-tauri/installer.conf.json` NSIS kurulumunu üretir: `target/release/bundle/nsis/KONI_<sürüm>_x64-setup.exe`. Kurulum kullanıcı başınadır (`%LOCALAPPDATA%\KONI`, yönetici izni yok); bu yüzden yt-dlp kendini güncelleyebilir. DLL'ler exe'nin yanına, `NOTICE.md`, `LICENSES/`, `source/` (mpv + FFmpeg kaynakları), `BUILDINFO.txt`, `REBUILD.md` de kurulum klasörüne gider (LGPL). Kaldırıcı Ayarlar → Uygulamalar'da; önbellek ve loglar (`%LOCALAPPDATA%\app.koni.desktop`) kaldırıcının "uygulama verisini sil" seçeneğiyle silinir. Kurulum imzasızdır: SmartScreen "Yine de çalıştır" ister.

Sürüm çıkarma otomatik: `tauri.conf.json` `version`'ını (ve `client/Cargo.toml` workspace sürümünü) artır, `docs/releases/vX.Y.Z.md` notunu yaz, main'e merge et. `release` iş akışı (`.github/workflows/release.yml`; main'de sürüm değişince, ya da `v*` etiketi / elle) `vX.Y.Z` etiketini ve release'i kendisi oluşturur, LGPL libmpv'yi `release.yml` içindeki `LIBMPV_RELEASE` sürümünden (şu an `libmpv-v0.40.0-r2`) indirir, bu betiği `-Portable` ile çalıştırır ve kurulum + taşınabilir zip'i GitHub Release'e (ön sürüm) koyar.

Kurulu uygulama kendini günceller: iş akışı kurulumu imzalar ve public `Savas07/koni-releases` reposuna yayınlar, uygulama açılışta oradaki `latest.json`'a bakar. Ayrıntı, imza anahtarı ve yedeği: [docs/UPDATES.md](../docs/UPDATES.md).

## Oynatıcı API'si (senkron motoru için)
`koni_player::Player` (Send + Sync): `load`, `load_stream` (ayrı ses + başlık, `loadfile … replace -1 <seçenekler>`, mpv ≥ 0.38), `play`, `pause`, `set_pause`, `toggle_pause`, `seek_absolute` (absolute+exact), `set_speed`, `time_pos`, `duration`, `is_paused`, `speed`, `demuxer_cache_duration`, `paused_for_cache`, `eof_reached`, `show_text`, `snapshot`, `wait_event` (tek thread'den).

## IPC komutları
`player_load {src}` → başlık, `player_play`, `player_pause`, `player_toggle_pause`, `player_seek {seconds}`, `player_set_speed {speed}`, `player_set_volume {volume}` (0–100), `player_add_subtitle {path}`, `player_sub_delay {delta}` (saniye, OSD'de gösterir), `player_state` → `{timePos, duration, paused, speed, seeking, pausedForCache, eofReached}`, `set_layout {layout: {mode, rightColDip, bottomBarDip, gapDip}}`, `set_fullscreen {on}`.

Oda: `sync_connect {url, username, roomName?, invite?}` (`invite` varsa katılır, yoksa oda kurar), `sync_play`, `sync_pause`, `sync_seek {seconds}`, `sync_continue_without {user}` (host/yardımcı host), `sync_set_cohost {user, on}` (host), `sync_set_policy {hostOnly}` (host), `sync_resync` ("Odayla Senkronla"), `sync_download {url}` ("Herkes İndirsin"), `sync_leave`. Olaylar: `sync://status` (4 Hz yerel durum, sapma, hız, RTT), `sync://server` (PROTOCOL.md §4 mesajları, pong hariç), `sync://local` (ileri atlama vb.), `sync://media` (oda medyasının yerel durumu: `resolving` | `downloading` | `ready` | `error` | `need_file`; indirmede `task`), `sync://link` (`online` | `reconnecting` + deneme sayısı), `sync://closed` (hata metni veya `null`).

Hesap (PROTOCOL.md §3.0): `account_get {url}` → kayıtlı hesap adı ya da `null`, `account_login {url, username, password}` → ad, `account_register {url, code, username, password}` → ad, `account_logout {url}`. Oturum belirteci sunucu adresine göre işletim sistemi anahtar zincirinde (`keyring`, Windows Credential Manager: `app.koni.desktop`) tutulur, şifre saklanmaz. `sync_connect` bu adres için belirteç varsa onu gönderir. Sunucu `auth_required` derse UI giriş formunu açar; `bad_token`'da belirteç silinir.

UI kısayolları: Boşluk oynat/durdur, `g`/`h` altyazı gecikmesi −/+50 ms, Esc tam ekrandan çık. Dosya/altyazı pencereye sürüklenip bırakılabilir (`.srt .ass .ssa .vtt .sub` altyazı olarak eklenir; "Aç" kutusuna altyazı yolu da yazılabilir). Ses düzeyi `localStorage` (`koni.volume`) içinde saklanır.

## Medya
"Aç" kutusu (`player_load`):
- **Web adresi**: doğrudan medya (`.mp4`, `.m3u8`, `.webm`, `.mkv`, `.mov`) olduğu gibi açılır; diğer her http(s) sayfası (YouTube ve yt-dlp'nin desteklediği VOD siteleri) `yt-dlp -J -f "bv*[height<=1080]+ba/b"` ile çözülür. Odadayken adres odaya `cmd media` olarak gider; her istemci (gönderen dahil) `state.media`'yı görünce adresi **kendisi** çözer ve yükler (stream adresleri çözen IP'ye bağlı). Canlı yayın reddedilir.
- **Yerel dosya**: yüklenir, odadaysa yalnızca dosya adı bildirilir. Diğerleri "kendi kopyanı aç" uyarısı alır; aynı adlı dosyayı açınca hazır olurlar.

**Herkes İndirsin** (`sync_download`, PROTOCOL.md §3.8): oda medyası `download` olur, her istemci sayfayı yt-dlp ile `%LOCALAPPDATA%\app.koni.desktop\downloads\<site>-<id>\` altına indirir ve saniyede bir `dl_progress` gönderir. ffmpeg olmadığı için video ve ses ayrı dosyalardır, mpv'ye `audio-file` ile birlikte verilir. Herkes hazır olunca Santral oda geri sayımını kendisi başlatır; öncesinde Oynat "hazır olanlarla başla" demektir. Önbellek 50 GB'ı aşınca en eski klasörler silinir. Aynı makinede iki istemci aynı klasörü `.lock` ile sırayla kullanır.

yt-dlp önce `koni.exe`'nin yanındaki `yt-dlp.exe`'de, sonra PATH'te aranır ([sürümler](https://github.com/yt-dlp/yt-dlp/releases)). Exe yanındaki kopya açılışta günde en çok bir kez arka planda `yt-dlp -U` ile güncellenir (`%APPDATA%\app.koni.desktop\ytdlp-update.stamp`).

## Oda senkronu
Oturum tek bir async görevdir: Santral WS bağlantısı (`ws://` / `wss://`), `koni-sync` motoru ve libmpv adaptörü (`MpvAdapter`, `koni_sync::Player`). Odadayken oynat/durdur/atla sunucuya `cmd` olarak gider; oynatıcıyı yalnızca motor sürer. Medya yüklenince oda konumuna hizalanır (bkz. Medya); DETACHED iken hizalanmaz, "Odayla Senkronla" beklenir.

Bağlantı kopması: 8 sn mesaj gelmezse bağlantı ölü sayılır, 70 sn boyunca `resume` denenir (PROTOCOL.md §2); oynatma bu sırada yerelde sürer. Döngünün kendisi 2 sn'den uzun takıldıysa (mpv dosya açarken) bu sessizlik bağlantıya yorulmaz. Tam ekranda etkinlik ve sohbet satırları mpv OSD'sinde gösterilir.

Santral: varsayılan adres `src/lib/server.ts`'te (`VITE_KONI_SERVER` ile derlemede değişir); kullanıcı giriş ekranındaki "Kendi sunucunu kur" ile kendi sunucusunu yazar (varsayılan adres ekranda gösterilmez). Kurulum: [docs/SELF-HOSTING.md](../docs/SELF-HOSTING.md). Yerel deneme: `server/` içinde `go run ./cmd/koni-server -addr 127.0.0.1:8080`, iki istemcide `ws://localhost:8080/ws`. Her oturum `%LOCALAPPDATA%\app.koni.desktop\logs\sync-<ms>.csv` dosyasına tick başına bir satır yazar (sapma, hız, RTT, ofset, geri seek sayacı); Faz 0 ölçümü bu dosyalardan yapılır.

Win32 çağrıları ana thread'de yapılır (`run_on_main_thread`). Tam ekranda ana pencere `WS_POPUP` olur ve video tüm alanı kaplar; Esc ile kokpite dönülür.
