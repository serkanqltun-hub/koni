# KONİ — İstemci

> [PLAN.md](../PLAN.md)'nin parçası. Bölüm numaraları korunmuştur; kod ve dokümanlardaki `§4.4` gibi atıflar bu numaralara işaret eder.

## 3. İstemci Detayı

### 3.1 Kokpit Modeli
Saydam webview'ı native video üstüne bindirmek her platformda kırılgandır (WebView2 hosting modları, WKWebView saydamlığı, compositor farkları). v0.2'de bu yaklaşım iptal edildi. Video ve web UI **yan yana, çakışmayan dikdörtgenlerdir**.

```
┌──────────────────────────────────────────────┬───────────────────────┐
│                                              │ ODA: Cuma Gecesi  🟢  │
│                                              │ Host: Serkan          │
│        NATIVE mpv VIDEO (child HWND)         │ ── Kullanıcılar ──    │
│        16:9, sol üst kutu                    │ Serkan  ▶ 01:24:10    │
│                                              │ Coni    ▶ +12 ms      │
│                                              │ Furkan  ⏳ %72 ▓▓▓▓░  │
│                                              │ ── Etkinlik ──        │
│                                              │ Serkan durdurdu       │
│                                              │   01:24:10            │
│                                              │ Coni: cips geldi 🍿   │
├──────────────────────────────────────────────┤ [mesaj yaz...]        │
│ ◀◀ ▶ ▶▶  ━━━━━━━●━━━━━━━━  01:24:10 / 02:03:00 │ 🔊 ━━━  CC TR ▾  ⛶  │
└──────────────────────────────────────────────┴───────────────────────┘
```

**Ekran modları:**
| Mod | Video kutusu | UI |
|---|---|---|
| Oynatma (kokpit) | Sol/üst canvas (native oynatıcıya ayrılmış) | Sağ kolon + alt bar (web kontrolleri) |
| Tam ekran | Tüm ekran | Gizli. Bildirimler mpv OSD'de (`show-text`), kısa süreli |
| Hub/Lobi, Keşif, Hazırlık (indirme) | Gizli (`ShowWindow(SW_HIDE)`) veya küçük önizleme | Tüm alan Svelte |
| Müzik Lounge | Gizli, ya da visualizer için küçük kutu | Tüm alan Svelte |

**Bildirimler**: "Serkan durdurdu: 01:24:10" gibi olaylar sağ paneldeki **etkinlik akışına** düşer. Bekleme durumları ("Furkan bekleniyor... [Buffer: %65]") hem sağ panelde hem video kutusunun hemen altındaki alt barda rozet olarak görünür. Tam ekrandaysa mpv `show-text` ile OSD mesajı çıkar. Ayrı bir overlay katmanına gerek yok.

### 3.2 Windows: Child HWND Gömme ve Pencere Yönetimi (Faz 0–1)
- Tauri ana penceresinin (WebView2'yi barındıran top-level HWND) altında `CreateWindowEx(WS_CHILD | WS_CLIPSIBLINGS, …)` ile bir **video child HWND** oluşturulur. mpv'ye `wid=<HWND>` verilir. mpv bu pencereye d3d11/gpu-next ile kendisi çizer.
- **Alan ayrımı / airspace**: Sol/üst canvas native oynatıcıya, sağ kolon ve alt bar web kontrollerine ayrılmıştır. Video HWND, kardeşleri arasında z-order'ın en üstünde durur ve webview'daki boş yer tutucu alanın üstünü kaplar. Web tarafında o alana hiçbir öğe çizilmez, bu yüzden **z-order/airspace çakışması sıfırdır**. Kural: dropdown, tooltip ve modal'lar sağ kolona veya yukarı doğru alt bara açılır. Tam pencere modal gerekiyorsa video geçici olarak gizlenir.
- **Konumlandırma doğrudan Rust'ta** (JS roundtrip yok):
  - Rust, ana pencereyi subclass eder (`SetWindowSubclass`) ve `WM_SIZE`, `WM_WINDOWPOSCHANGED`, `WM_DPICHANGED` mesajlarını yakalar. Video dikdörtgeni o anki istemci alanı ve **layout config**'ten hesaplanır, `SetWindowPos(..., SWP_NOACTIVATE)` aynı mesaj işleyicisinde uygulanır.
  - Neden: Pencere kenarından sürükleyerek boyutlandırmada Windows bir **modal size-move döngüsüne** girer. Pencere prosedürü mesaj almaya devam eder, ama webview → JS → IPC → Rust zinciri bu sırada gecikebilir ya da takılabilir. Konumu pencere prosedüründe hesaplamak videonun pencereyi kare kare izlemesini sağlar.
  - **Layout config** Svelte'den Rust'a seyrek gider: yalnızca mod değişiminde, panel daraltma/genişletmede ve ayar değişiminde, her frame değil. Örnek: `{ "mode": "cockpit", "rightColDip": 340, "bottomBarDip": 72, "gapDip": 0 }`. Değerler DIP (mantıksal piksel) cinsindendir, Rust `GetDpiForWindow` ile fiziksel piksele çevirir.
  - **DPI**: Per-Monitor V2 DPI awareness (Tauri/WebView2 ile uyumlu olduğu [doğrulanmalı]). `WM_DPICHANGED`'de Windows'un önerdiği yeni pencere dikdörtgeni uygulanır ve video yeniden hesaplanır. Webview kendi CSS ölçeğini ayrıca günceller. İki tarafın aynı DIP değerlerini kullanması hizalamayı garanti eder.
  - Minimize/restore'da child HWND ebeveyni otomatik izler. Video kutusunun en-boy oranı sabit değildir, mpv letterbox yapar.
- **Klavye/fokus**: mpv'nin kendi input'u kapatılır (`input-default-bindings=no`, `input-vo-keyboard=no`). Kokpitte kısayollar webview'da yakalanır ve Rust üzerinden mpv'ye iletilir. Video kutusuna tıklandığında fokus webview'a geri verilir. Tam ekranda tuşları Rust kendi pencere prosedüründe (`WM_KEYDOWN`) karşılar.
- **Tam ekran**: Webview'ın borderless kokpit layout'u kaldırılır (webview gizlenir). Ana pencere native hook'larla stil değiştirir (`WS_POPUP`, `MonitorFromWindow` ile hedef monitör, varsayılan birincil ekran) ve child HWND ekranı tamamen doldurur. Çıkışta (Esc/çift tık) pencere stili ve dikdörtgeni geri yüklenir, kokpit layout config yeniden uygulanır.

### 3.3 Platform Soyutlaması: `VideoSurface`
Rust çekirdeğinde platformdan bağımsız bir trait, video yüzeyinin yaşam döngüsünü (oluşturma, resize, fullscreen, destroy) tanımlar. Svelte kontrolleri, layout config, Tauri IPC komutları ve WebSocket protokolü platformdan bağımsızdır. macOS'a geçişte bunlar **değişmez**.

```rust
// pseudo-imza
trait VideoSurface {
    fn create(parent: RawWindowHandle, layout: &LayoutConfig) -> Result<Self> where Self: Sized;
    fn mpv_target(&self) -> MpvTarget;            // Wid(i64) | RenderContext(..)
    fn apply_layout(&mut self, layout: &LayoutConfig);       // Svelte'den seyrek
    fn on_parent_resized(&mut self, client: PhysicalSize, scale: f64); // WM_SIZE / DPI
    fn set_visible(&mut self, visible: bool);     // Hub/Lounge/Hazırlık modları
    fn enter_fullscreen(&mut self) -> Result<()>;
    fn exit_fullscreen(&mut self) -> Result<()>;
    fn destroy(self);
}
```
- **Windows (Faz 1)**: `Win32ChildSurface`: child HWND + `wid`, pencere subclass'ı ile resize takibi.
- **macOS (Faz 2)**: `MacSurface`. İki seçenek var:
  - **M1, NSView wrapper**: Content view'a webview'ın kardeşi olarak bir NSView eklenir ve `wid` olarak mpv'ye verilir. mpv'nin macOS `wid` desteği kırılgan olabilir [doğrulanmalı].
  - **M2, `mpv_render_context`**: Kendi NSView'umuzda OpenGL layer veya mpv'nin desteklediği ölçüde Metal tabanlı bir yol. IINA bu yolu kullanır [doğrulanmalı].

  Faz 2 başında M1 için 1 haftalık bir deneme yapılır, kararsız kalırsa M2'ye geçilir. Resize kaynağı `NSWindowDelegate`/`viewDidEndLiveResize` ve `setFrameSize` olur. AppKit'te y ekseni alttan başlar, ölçek `backingScaleFactor` ile alınır.

### 3.4 Linux (ileride, opsiyonel)
X11'de `wid` gömme Windows'a benzer şekilde çalışır. Wayland'de `wid` yoktur, render API gerekir. Talep olursa ele alınır.

### 3.5 libmpv Entegrasyonu
- Rust'ta `libmpv2`/`libmpv-sys` crate'leri ile C API kullanılır [crate bakım durumu doğrulanmalı]. Windows'ta `mpv-2.dll` (veya `libmpv-2.dll`) pakete eklenir.
- **Observe edilen property'ler**: `time-pos`, `pause`, `seeking`, `paused-for-cache`, `cache-buffering-state`, `demuxer-cache-duration`, `demuxer-cache-state`, `eof-reached`, `speed`, `duration`, `sub-delay`, `track-list`.
- **Komutlar**: `loadfile`, `seek <abs> absolute+exact`, `set pause`, `set speed`, `sub-add`, `set sub-delay`, `show-text`.
- **Yankı önleme**: Uzaktan gelen komut uygulanırken oluşan property değişimleri sunucuya geri gönderilmez. Bunun için bir beklenen-değişiklik kuyruğu ve `cid`/`ack` eşleşmesi kullanılır ([sync.md §4](sync.md), [PROTOCOL.md](../PROTOCOL.md)).
- Önerilen ayarlar: `hwdec=auto-safe`, `vo=gpu-next`, `hr-seek=yes`, `audio-pitch-correction=yes`, `keep-open=yes`, `cache=yes`, `demuxer-max-bytes=200MiB`.

### 3.6 UI Çatısı: Svelte 5 + Vite + TypeScript
Svelte'in bundle'ı küçük ve runtime yükü düşük. Reaktif store'lar WS oda state'ine doğal oturur, boilerplate da az. Konfeti ve swipe için `svelte/motion` + `canvas-confetti` yeterli. Karanlık tema varsayılan. React'e ancak ekip deneyimi gerektirirse geçilir.

### 3.7 yt-dlp Sidecar
- Tauri `externalBin` ile `yt-dlp.exe` ve `ffmpeg.exe` gömülür.
- **Güncelleme**: Açılışta günde en fazla bir kez GitHub Releases kontrol edilir. Yeni sürüm **SHA2-256SUMS** ile doğrulanır, uygulama veri dizinine atomik olarak konur. Başarısızlıkta gömülü sürüme dönülür. Hata ekranında "yt-dlp'yi güncelle" butonu bulunur.
- YouTube için harici JS runtime (ör. Deno) gereksinimi duyurulmuştu [doğrulanmalı]. Gerekiyorsa o da sidecar olarak eklenir.
- **Kullanım**: `yt-dlp -J` ile JSON alınır. Format seçimini KONİ yapar ve odaya **aynı format seçici** önerilir (`bv*[height<=1080]+ba/b`). Oynatma için mpv'ye ayrı video+ses URL'leri `--audio-file` ile, ya da tek birleşik URL verilir. mpv'nin `ytdl_hook`'u yedek yoldur.
- Çerez: opsiyonel `--cookies-from-browser`, tamamen yerel.
