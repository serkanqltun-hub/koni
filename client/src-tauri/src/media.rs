//! Medya açma: yerel dosya ya da web adresi (yt-dlp ile çözülür).
//!
//! yt-dlp'nin verdiği stream adresleri çözen istemcinin IP'sine bağlıdır; bu yüzden
//! odada her istemci aynı sayfa adresini kendisi çözer (docs/design/client.md §3.7).

use crate::i18n::trs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use koni_player::Player;
use koni_resolver::{
    classify, resolve_with, Alt, ProcessRunner, ResolveError, UrlKind, YtDlpConfig,
};
use koni_sync::protocol::SubFile;

/// Kullanıcının "Aç" kutusuna yazdığı şey.
pub enum Source {
    /// http(s) adresi: doğrudan medya ya da yt-dlp ile çözülecek sayfa.
    Web(String),
    /// Yerel dosya yolu.
    File(String),
}

impl Source {
    pub fn parse(input: &str) -> Result<Source, String> {
        let s = input.trim();
        if s.is_empty() {
            return Err(trs("dosya yolu veya URL boş", "file path or URL is empty").into());
        }
        match classify(s) {
            Ok(_) => Ok(Source::Web(s.to_owned())),
            Err(_) if s.starts_with("http://") || s.starts_with("https://") => {
                Err(tr!("geçersiz URL: {s}", "invalid URL: {s}"))
            }
            Err(_) => Ok(Source::File(s.to_owned())),
        }
    }
}

/// Odada paylaşılan ad: yolun son parçası (yerel yol sunucuya gitmez).
pub fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}

/// OpenSubtitles `moviehash`: dosya boyutu + ilk ve son 64 KiB'ın 64 bitlik
/// little-endian sözcüklerinin taşmalı toplamı, 16 hane küçük harf hex.
pub fn movie_hash(path: &Path) -> std::io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    const CHUNK: u64 = 64 * 1024;
    let mut f = std::fs::File::open(path)?;
    let size = f.metadata()?.len();
    if size < CHUNK * 2 {
        return Err(std::io::Error::other("dosya çok küçük"));
    }
    let mut sum = size;
    let mut buf = vec![0u8; CHUNK as usize];
    for start in [0, size - CHUNK] {
        f.seek(SeekFrom::Start(start))?;
        f.read_exact(&mut buf)?;
        for w in buf.as_chunks::<8>().0 {
            sum = sum.wrapping_add(u64::from_le_bytes(*w));
        }
    }
    Ok(format!("{sum:016x}"))
}

/// Oynatıcıya yüklenmeye hazır web akışı.
#[derive(Clone)]
pub struct Stream {
    pub video: String,
    pub audio: Option<String>,
    pub title: String,
    /// Yakalanan akış: geldiği sayfa (HTTP Referer). yt-dlp akışlarında yok.
    pub referer: Option<String>,
    /// Dosya açılınca eklenecek altyazılar (yt-dlp ya da yakalayıcı).
    pub subs: Vec<SubFile>,
    /// yt-dlp: seçilebilecek kaliteler ve dublaj dilleri (`tracks`).
    pub videos: Vec<Alt>,
    pub audios: Vec<Alt>,
    /// yt-dlp kapağı: görüntüsüz seste (müzik) albüm kapağı olarak gösterilir.
    pub cover: Option<String>,
    /// Yakalanan akış içeriğinden HLS: FFmpeg'e biçim söylenir (`sniff_hls`).
    pub hls: bool,
}

impl Stream {
    /// Yalnızca dosya(lar): alternatif iz yok.
    pub fn plain(video: String, audio: Option<String>, title: String) -> Stream {
        Stream {
            video,
            audio,
            title,
            referer: None,
            subs: Vec::new(),
            videos: Vec::new(),
            audios: Vec::new(),
            cover: None,
            hls: false,
        }
    }
}

/// Web adresi çözülemedi. `Unsupported`: yt-dlp siteyi tanımıyor; sayfadan akış
/// yakalanabilir (`sniffer`).
#[derive(Debug)]
pub enum WebError {
    Unsupported(String),
    Other(String),
}

impl std::fmt::Display for WebError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebError::Unsupported(m) | WebError::Other(m) => f.write_str(m),
        }
    }
}

impl From<WebError> for String {
    fn from(e: WebError) -> String {
        e.to_string()
    }
}

/// `ResolveError` metni arayüz dilinde (çözücü crate'i yalnızca Türkçe yazar).
pub fn resolve_err(e: &ResolveError) -> String {
    if !crate::i18n::en() {
        return e.to_string();
    }
    match e {
        ResolveError::InvalidUrl(u) => format!("invalid URL: {u}"),
        ResolveError::Unsupported(u) => format!("this site isn't supported yet: {u}"),
        ResolveError::YtDlpNotFound(m) => format!("yt-dlp not found ({m})"),
        ResolveError::Timeout(s) => format!("yt-dlp didn't respond within {s} s"),
        ResolveError::Unavailable => "video unavailable (deleted, private or doesn't exist)".into(),
        ResolveError::Private => "video is private or requires sign-in".into(),
        ResolveError::GeoBlocked => "video is blocked in this region".into(),
        ResolveError::AgeRestricted => "age-restricted video, requires sign-in".into(),
        ResolveError::LiveUnsupported => "live streams aren't supported in synced watching".into(),
        ResolveError::Failed(m) => format!("yt-dlp failed: {m}"),
        ResolveError::BadOutput(m) => format!("couldn't parse yt-dlp output: {m}"),
        ResolveError::Io(e) => format!("I/O error: {e}"),
    }
}

/// Son yt-dlp çözümleri. "Aç" önce yerelde çözer (desteklenmiyorsa yakalayıcı açılır),
/// hemen ardından oda medyası aynı adresi yükler; müzikte sıradaki şarkı çalan bitmeden
/// çözülür (`prefetch`). İkisinde de yt-dlp (birkaç sn) ikinci kez beklenmesin.
/// YouTube akış adresleri saatlerce geçerli; süre yine de kısa tutulur.
static CACHE: Mutex<Vec<(String, Instant, Stream)>> = Mutex::new(Vec::new());
const CACHE_TTL: Duration = Duration::from_secs(20 * 60);
const CACHE_MAX: usize = 16;
/// Aynı adres için süren çözüm: ikinci istek yeni yt-dlp açmaz, ilkini bekler (ön
/// çözüm sürerken şarkı sırası gelirse).
static INFLIGHT: Mutex<Vec<(String, Arc<Mutex<()>>)>> = Mutex::new(Vec::new());

fn cached(url: &str) -> Option<Stream> {
    let mut c = CACHE.lock().unwrap();
    c.retain(|(_, at, _)| at.elapsed() < CACHE_TTL);
    c.iter().find(|(u, ..)| u == url).map(|(.., s)| s.clone())
}

fn remember(url: &str, s: &Stream) {
    let mut c = CACHE.lock().unwrap();
    c.retain(|(u, at, _)| u != url && at.elapsed() < CACHE_TTL);
    if c.len() >= CACHE_MAX {
        c.remove(0);
    }
    c.push((url.to_owned(), Instant::now(), s.clone()));
}

/// Adresin çözüm kilidi; kullanılmayanlar atılır.
fn inflight(url: &str) -> Arc<Mutex<()>> {
    let mut v = INFLIGHT.lock().unwrap();
    v.retain(|(_, l)| Arc::strong_count(l) > 1);
    if let Some((_, l)) = v.iter().find(|(u, _)| u == url) {
        return l.clone();
    }
    let l = Arc::new(Mutex::new(()));
    v.push((url.to_owned(), l.clone()));
    l
}

/// Müzik: sıradaki şarkıyı şimdiden çözer, sırası gelince hemen açılır. Bloklar.
pub fn prefetch(url: &str) {
    if let Err(e) = resolve_web(url, None, "", &[]) {
        eprintln!("[koni] ön çözüm başarısız ({url}): {e}");
    }
}

/// Web adresini çözer. Bloklar (yt-dlp alt süreci): async bağlamdan
/// `spawn_blocking` ile çağrılmalı. `referer` verilmişse adres yakalanmış doğrudan
/// akıştır, yt-dlp'ye gitmez; `title` ve `subs` yalnızca doğrudan akışlarda kullanılır.
pub fn resolve_web(
    url: &str,
    referer: Option<&str>,
    title: &str,
    subs: &[SubFile],
) -> Result<Stream, WebError> {
    if matches!(classify(url), Ok(UrlKind::Direct(_))) || referer.is_some() {
        let title = if title.is_empty() {
            file_name(url)
        } else {
            title.to_owned()
        };
        return Ok(Stream {
            referer: referer.map(str::to_owned),
            subs: subs.to_vec(),
            hls: referer.is_some_and(|r| sniff_hls(url, r)),
            ..Stream::plain(url.to_owned(), None, title)
        });
    }
    let lock = inflight(url);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = cached(url) {
        return Ok(s);
    }
    let s = resolve_with(url, &ytdlp_config(), &ProcessRunner).map_err(|e| match e {
        ResolveError::Unsupported(_) => WebError::Unsupported(resolve_err(&e)),
        ResolveError::YtDlpNotFound(_) => WebError::Other(tr!(
            "{}. yt-dlp.exe'yi koni.exe'nin yanına koy ya da PATH'e ekle \
             (https://github.com/yt-dlp/yt-dlp/releases)",
            "{}. Put yt-dlp.exe next to koni.exe or add it to PATH \
             (https://github.com/yt-dlp/yt-dlp/releases)",
            resolve_err(&e)
        )),
        e => WebError::Other(resolve_err(&e)),
    })?;
    let s = Stream {
        subs: s
            .subs
            .into_iter()
            .map(|x| SubFile {
                url: x.url,
                lang: x.lang,
                title: x.title,
            })
            .collect(),
        videos: s.videos,
        audios: s.audios,
        cover: s.thumbnail,
        ..Stream::plain(s.video_url, s.audio_url, s.title)
    };
    remember(url, &s);
    Ok(s)
}

/// Yakalanan akışlar tarayıcı gibi istenir: bazı CDN'ler mpv'nin varsayılan
/// user-agent'ını ya da Referer'sız isteği reddeder.
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0";

/// Yakalanan akış HLS mi: adres `.m3u8` ile bitmiyorsa ilk baytına bakılır. Siteler
/// manifesti sık sık `text/html` ya da `.txt` kılığında, uzantısız adresten verir;
/// FFmpeg 7.1 uzantısı da türü de standart olmayan HLS'yi tanımaz ("Not detecting
/// m3u8/hls with non standard extension") ve mpv dosyayı hemen bitirir. Bloklar.
fn sniff_hls(url: &str, referer: &str) -> bool {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let known = [
        ".m3u8", ".m3u", ".mpd", ".mp4", ".m4v", ".mkv", ".webm", ".mov",
    ];
    if known.iter().any(|e| path.ends_with(e)) {
        return false;
    }
    let req = crate::addons::http()
        .get(url)
        .header(reqwest::header::REFERER, referer)
        .header(reqwest::header::USER_AGENT, BROWSER_UA)
        .timeout(std::time::Duration::from_secs(8));
    tauri::async_runtime::block_on(async move {
        let mut res = req.send().await.ok()?.error_for_status().ok()?;
        let head = res.chunk().await.ok()??;
        let text = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&head[..]);
        Some(text.trim_ascii_start().starts_with(b"#EXTM3U"))
    })
    .unwrap_or(false)
}

pub fn load(mpv: &Player, s: &Stream) -> Result<(), String> {
    // Müzik modunda görüntü gizli: sesi ayrı akış olan parçada (YouTube) yalnızca ses
    // açılır. Yoksa 1080p görüntü boşuna iner; tampon ona yetişemeyince şarkı durur,
    // odada herkes bekler. Sinema'ya geçilince görüntü eklenir (`lounge`).
    let audio_only = s.referer.is_none() && crate::lounge::music_view();
    let owned;
    let s = match &s.audio {
        Some(a) if audio_only => {
            crate::lounge::set_video_later(Some((a.clone(), s.video.clone())));
            owned = Stream {
                video: a.clone(),
                audio: None,
                ..s.clone()
            };
            &owned
        }
        _ => {
            crate::lounge::set_video_later(None);
            s
        }
    };
    let mut extra = vec![("user-agent", BROWSER_UA)];
    if let Some(r) = &s.referer {
        extra.push(("referrer", r.as_str()));
        // Yakalanan HLS'de parçalar sık sık .jpg/.png/.txt kılığında gelir; FFmpeg'in
        // uzantı denetimi bunları reddeder ve video birkaç saniyede kesilir.
        extra.push(("demuxer-lavf-o", "allowed_extensions=ALL,extension_picky=0"));
    }
    if s.hls {
        extra.push(("demuxer-lavf-format", "hls"));
    }
    mpv.load_stream(&s.video, s.audio.as_deref(), &s.title, &extra)
        .map_err(|e| e.to_string())?;
    crate::tracks::set_current(s);
    Ok(())
}

/// Exe'nin yanında paketlenmiş `yt-dlp.exe` (varsa).
#[cfg(not(target_os = "macos"))]
fn bundled_ytdlp() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("yt-dlp.exe")))
        .filter(|p| p.exists())
}

/// macOS: uygulama verisi klasörü (`spawn_ytdlp_update` ayarlar).
#[cfg(target_os = "macos")]
static DATA_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// macOS: `KONI.app/Contents/Resources/yt-dlp` ilk kullanımda uygulama verisine
/// kopyalanır. Paketin içindeki dosya değişirse uygulamanın imzası bozulur; kopya ise
/// `yt-dlp -U` ile kendini güncelleyebilir.
#[cfg(target_os = "macos")]
fn bundled_ytdlp() -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let dest = DATA_DIR.get()?.join("bin").join("yt-dlp");
    if dest.exists() {
        return Some(dest);
    }
    let src = std::env::current_exe()
        .ok()?
        .parent()?
        .join("../Resources/yt-dlp");
    if !src.exists() {
        return None;
    }
    std::fs::create_dir_all(dest.parent()?).ok()?;
    let tmp = dest.with_extension("tmp");
    std::fs::copy(&src, &tmp).ok()?;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).ok()?;
    // İndirilen uygulamanın karantina işareti kopyaya geçmesin.
    let _ = std::process::Command::new("/usr/bin/xattr")
        .args(["-d", "com.apple.quarantine"])
        .arg(&tmp)
        .output();
    std::fs::rename(&tmp, &dest).ok()?;
    Some(dest)
}

/// Önce exe'nin yanındaki `yt-dlp.exe`, yoksa PATH'teki `yt-dlp`.
pub fn ytdlp_bin() -> String {
    bundled_ytdlp().map_or_else(|| "yt-dlp".into(), |p| p.display().to_string())
}

fn ytdlp_config() -> YtDlpConfig {
    YtDlpConfig {
        binary: ytdlp_bin(),
        ..YtDlpConfig::default()
    }
}

/// Siteler sık değiştiği için paketlenmiş yt-dlp günde en fazla bir kez arka planda
/// kendini günceller (`yt-dlp -U`: resmi sürümü indirir ve imza/sağlama toplamını
/// kendisi doğrular). PATH'teki yt-dlp kullanıcınındır, dokunulmaz.
pub fn spawn_ytdlp_update(state_dir: std::path::PathBuf) {
    const EVERY: std::time::Duration = std::time::Duration::from_secs(24 * 3600);
    #[cfg(target_os = "macos")]
    let _ = DATA_DIR.set(state_dir.clone());
    let Some(bin) = bundled_ytdlp() else { return };
    let stamp = state_dir.join("ytdlp-update.stamp");
    let fresh = std::fs::metadata(&stamp)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < EVERY);
    if fresh {
        return;
    }
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new(&bin);
        cmd.args(["-U", "--no-config"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        match cmd.output() {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                eprintln!("[koni] yt-dlp -U: {}", text.trim());
                if out.status.success() {
                    let _ = std::fs::create_dir_all(&state_dir);
                    let _ = std::fs::write(&stamp, text.as_bytes());
                }
            }
            Err(e) => eprintln!("[koni] yt-dlp güncellenemedi: {e}"),
        }
    });
}
