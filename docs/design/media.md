# KONİ — Altyazı, İndirme ve Müzik

> [PLAN.md](../PLAN.md)'nin parçası. Bölüm numaraları korunmuştur; kod ve dokümanlardaki `§4.4` gibi atıflar bu numaralara işaret eder.

## 6. Altyazı, İndirme Modu ve Müzik Lounge

### 6.1 Ortak İndirme Bariyeri ("Herkes İndirsin") — çekirdek akış
- Host medyayı seçer ve Santral `download {taskId, url, format}` yayınlar. Kokpit "Hazırlık" moduna geçer: video gizlenir, tam ekran ilerleme kartları açılır.
- Her istemci yt-dlp'yi `--newline --progress-template` ile çalıştırır, satırları ayrıştırır ve 1 sn'de bir raporlar:
  ```json
  { "t": "dl_progress", "taskId": "d7", "pct": 72.4, "speed": 8.1e6, "eta": 95, "state": "downloading" }
  ```
- UI'da kişi başına bar görünür (Coni %100, Furkan %72). Durumlar: `queued/downloading/merging/ready/failed`.
- **Bariyer**: `ready` için dosyanın tamamlanmış ve ffprobe ile süresinin okunmuş olması gerekir. Herkes hazır olunca planlı başlangıç yapılır. Host "hazır olanlarla başla" diyebilir; geç kalan dosyası bitince odaya katılıp hizalanır.
- Aynı dosya kontrolü süre ±0.5 sn ile yapılır, çünkü bayt-bayt aynılık garanti değildir. Önbellek kotası (ör. 50 GB) ve LRU temizliği uygulanır.
- **Faz 1 uygulaması (PROTOCOL p0.4)**: Host ayrı bir mesaj yerine `cmd media` (`kind: "download"`) gönderir; Santral `media.task` kimliğini atar. "Hazırlık" ekranı yok: kişi başına bar üye listesinde görünür. ffmpeg paketlenmediği için video ve ses ayrı dosyalar olarak indirilir (`-f "<video>,<ses>"`) ve mpv'ye `audio-file` ile verilir. Birleştirme yapılmaz, bu yüzden `merging` durumu yok. `ready`, yt-dlp dosyaları bitirip yolunu bildirince verilir; ffprobe süre kontrolü ve aynı dosya karşılaştırması yapılmıyor (herkes aynı sayfayı aynı biçim seçimiyle indiriyor). Önbellek `%LOCALAPPDATA%\app.koni.desktop\downloads\<site>-<id>`, kota 50 GB, LRU. Yarım kalan indirme aynı sayfa yeniden istenince `.part`'tan sürer.

### 6.2 Altyazı
- Faz 1: gömülü/yerel altyazı, sürükle-bırak `.srt/.ass`, **g/h ile ±50 ms** (`sub-delay`, varsayılan yerel, "odaya uygula" seçeneğiyle). mpv'nin kendi kısayolları kapalı olduğu için çakışma yok.
- Faz 3: **OpenSubtitles REST API** (`moviehash` ile: boyut + ilk/son 64 KiB). Hash yalnızca yerel/indirilmiş dosyada çalışır; web stream'de IMDb/isim araması yapılır. Ücretsiz hesapta günlük indirme kotası var [doğrulanmalı]. Stremio altyazı addon'ları (`/subtitles/{type}/{id}.json`) [doğrulanmalı]. Tek tık TR/EN. Host seçince altyazı metni WS ile odaya yayınlanır.
- **Uygulanan (p0.10)**: Santral `server/subs` (OpenSubtitles REST v1; `KONI_OS_KEY`, isteğe bağlı `KONI_OS_USER`/`KONI_OS_PASS` ile günlük oturum). İstemci yerel dosyada `moviehash`'i kendisi hesaplar (`media::movie_hash`), akışta medya başlığından temizlenmiş adla arar. "Kalite · ses · altyazı" çekmecesinde **TR / EN**: host ya da yardımcı host için en iyi sonuç (dosyaya özel > insan çevirisi > indirme sayısı) otomatik seçilir; diğerleri listede. Seçilen altyazı Santral'da indirilip (UTF-8, Windows-1254 düzeltmesi) `subtitle` ile odaya yayınlanır, her istemci önbelleğe yazıp `sub-add` ile yükler ([PROTOCOL.md §3.10, §4.14](../PROTOCOL.md)). Stremio altyazı addon'ları yok.

### 6.3 Müzik / Radyo Lounge
- Video gizlenir, tüm alan Svelte'e kalır. Ses mpv'de çalar.
- Ortak kuyruk (Santral), sürükle-bırak sıralama ve "sonraki" oylaması. Eklenen şarkılar `proposals` tablosuna da yazılır (DJ rozeti).
- Kaynaklar: YouTube/SoundCloud (yt-dlp) ve yerel FLAC/MP3. Yerel dosya paylaşımı yok: Santral'a hiçbir dosya yüklenmez (aşağıdaki relay kaldırıldı).
- Kapak: gömülü etiket / yt-dlp thumbnail / Cover Art Archive.
- **Visualizer**: Ses webview'da değil mpv'de çaldığı için Web Audio kullanılamaz. Pratik yol: mpv `lavfi-complex` ile `showcqt`/`showwaves` çıktısını küçük video kutusunda göstermek (sesle birebir senkron). Seek bar için ffmpeg ile önceden statik dalga formu (peaks JSON) üretilir.
- **LRC sözler**: LRCLIB (açık API, synced LRC). Satır vurgulama `time-pos`'a bağlı.
- Şarkı geçişlerinde planlı başlangıç kullanılır. Hedef tolerans < 200 ms.
- **Müzikte hız düzeltmesi**: Pitch correction artefaktları müzikte konuşmadan daha çok duyulur. Bu yüzden Lounge'da ölü bant genişletilir (|sapma| < 150 ms'de düzeltme yok) ve hız aralığı daraltılır (0.98–1.02). Biriken sapma tercihen **şarkı geçişinde** yeni anchor ile sıfırlanır. Bu da geri seek değildir, çünkü yeni şarkı baştan başlar.
- **Uygulanan (p0.11, kısmi)**: Santral'da ortak kuyruk (`server/hub/queue.go`): herkes link ekler (istemci önce çözer, başlık görünür), "sıradaki" oyu sırayı değiştirir (en çok oy, eşitlikte ilk eklenen). Bir istemcinin mpv'si sona gelince (`eof-reached`) `track_end` gider; Santral kuyruğun başını oda medyası yapar ve 3 sn sonra (herkes yüklesin diye) geri sayımı kendisi başlatır. İstemcide sağ rayda ♫ "Lounge · kuyruk" çekmecesi ve LRCLIB senkron sözler (başlıktan arama, `time-pos`'a göre satır vurgusu; LRCLIB `Access-Control-Allow-Origin: *` döner). Henüz yok: video gizleyen Lounge görünümü, yerel dosya relay'i, kapak, visualizer, dalga formu, sürükle-bırak sıralama, müziğe özel ölü bant/hız aralığı, dinleme süresi istatistiği.
- **Uygulanan (p0.12)**:
  - **Görünüm**: Video alanı kaldırılmaz; mpv native penceresi orada olduğu için müzikte o alan kapak ya da görselleştiriciyle dolar. Kuyruk, oylama ve sözler sağdaki ♫ çekmecesindedir. Ayrı bir Svelte Lounge sahnesi native pencerenin üstüne çizilemeyeceği için yapılmadı. Müzik algısı: ses izi var, (albüm kapağı dışında) görüntü izi yok (`client/src-tauri/src/lounge.rs`).
  - **Müzik modunda video gizlenir**: Oda (ya da tek başına) Müzik modundayken görüntülü parça da (ör. YouTube klibi) videosuz çalar. Video izi `lavfi-complex` grafiğinde `[vidN]nullsink` ile tüketilir, görüntü `showcqt`'tan gelir. Video izi grafikten tamamen çıkarılırsa ana demuxer okumayı bırakır: ses ayrı dosyadaysa (YouTube) `demuxer-cache-duration` 0 olur ve oda "takılıyor"da bekler. `vd-lavc-skipframe=all` ile çözümü atlamak oynatmayı durdurduğu için denenip bırakıldı. Sinema'ya dönünce grafik kalkar ve eski `vid`/`aid` geri seçilir; mpv bunları kendisi seçmez, ses de susar.
  - **Kapak**: Dosyanın gömülü kapağı varsa o gösterilir. Yoksa yt-dlp `thumbnail` adresi mpv `video-add … albumart` ile eklenir. Cover Art Archive kullanılmadı.
  - **Görselleştirici**: Kapak yoksa mpv `lavfi-complex` ile `showcqt` (1280×720, 30 fps, eksen yazısız) devreye girer. Görüntü sesle aynı zamanda üretilir ve sonraki dosyada temizlenir. LGPL libmpv derlemesinde 30 fps çıktıyla doğrulandı.
  - **Dalga formu**: ffmpeg ikilisi yok. İkinci, görüntüsüz bir libmpv sesi `ao=pcm` (untimed) ile 8 kHz mono s16'ya çözer. Sütun başına tepe değerinden 160 sütunluk, en yüksek sütunu 1 olacak şekilde ölçeklenmiş bir dizi çıkar ve seek çubuğunun arkasına çizilir. Yalnızca müzikte ve 20 dakikaya kadar çalışır. 3 dakikalık WAV 0,3 sn'de çözüldü (`waveform.rs`).
  - **Yerel dosya relay'i (kaldırıldı)**: 0.0.10–0.0.12'de ses dosyası Santral'ın RAM'ine yüklenip odaya çalınıyordu (200 MB/dosya, 1 GB toplam). Sunucu yükü yüzünden kaldırıldı; Santral dosya kabul etmez, kuyruğa yalnızca link girer.
  - **Müzikte senkron**: Müzik algılanınca motor `SyncConfig::lounge()` ayarına geçer: ±150 ms ölü bant, 0.98–1.02 hız aralığı. Film açılınca varsayılana döner.
  - **Dinleme süresi**: Oda medyası kuyruktan geldiyse süre izlemeye değil `listen` olayına yazılır (`stats.listenSec`). İstatistik panelinde "Lounge'da X müzik dinledin" olarak görünür.
  - **Çalma listeleri**: Spotify benzeri, bu bilgisayarda (`localStorage`, `playlists.svelte.ts`). Liste oluşturulur, yeniden adlandırılır, silinir. Şarkı eklemenin yolları: YouTube araması, link (YouTube/SoundCloud çalma listesi linki tüm şarkılarıyla, `yt-dlp --flat-playlist -J`, en fazla 500; gizli/silinmiş girdiler atlanır), "Çalanı ekle" ve kalp (Beğendiklerim). Sıralama ▲▼ ile yapılır; Tauri pencereye dosya bırakmayı yakaladığı için HTML sürükle-bırak çalışmaz. Çal / Karıştır / bir şarkıdan itibaren çal. Tek başına modda sıra yerelde çalar: parça sonu `eof-reached` ile görülür, mpv (keep-open) sonda duraklattığı için sıradaki oynatılarak açılır, açılamayan şarkı atlanır. Odada liste Lounge kuyruğuna azar azar eklenir: kuyrukta bu listeden en fazla 2 şarkın bekler, biri çalınca sıradaki girer. Böylece Santral'ın kişi başı 10 şarkı sınırına takılmaz, başkalarının şarkıları ve oylama araya girebilir. Listeden gelen şarkı yt-dlp listesinden geçtiği için `queue_add` onu yeniden çözmez.
  - Yapılmadı: Lounge kuyruğunda sürükle-bırak sıralama (sırayı "sıradaki" oyu belirliyor).
