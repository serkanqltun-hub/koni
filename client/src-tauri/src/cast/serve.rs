//! Ev ağına açık küçük HTTP sunucu: TV, oynatılan yerel dosyayı ve altyazıyı buradan okur.
//!
//! İlk gönderimde açılır, uygulama kapanınca kapanır. Yalnızca yerel ağ adreslerinden
//! gelen isteklere cevap verir; yol, uygulama her açıldığında değişen rastgele bir
//! belirteç taşır. Aynı anda yalnızca son hazırlanan medya sunulur.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Duration;

use crate::i18n::trs;

/// Sunulan içerik: yerel dosya, metin (altyazı) ya da TV'nin doğrudan açamadığı bir akış.
#[derive(Clone)]
pub enum Body {
    File(PathBuf),
    Text(String),
    /// Uzak `http(s)` akışı TV'ye bu bilgisayar üzerinden aktarılır: birçok TV `https`
    /// adresi açamaz ya da yanlış içerik türü yüzünden hata verir.
    Remote {
        url: String,
        referer: Option<String>,
    },
}

struct Entry {
    body: Body,
    mime: String,
}

struct Server {
    port: u16,
    token: String,
}

static SERVER: OnceLock<Server> = OnceLock::new();
static START: Mutex<()> = Mutex::new(());
/// Sunulan dosyalar: ad (`media.mp4`, `sub.vtt`, `sub.srt`) → içerik.
static FILES: Mutex<Option<HashMap<String, Entry>>> = Mutex::new(None);

/// Medyayı (`ext` uzantısıyla, `mime` türüyle) ve VTT altyazıyı sunar; önceki medya kalkar.
/// Altyazı SRT olarak da sunulur (DLNA TV'ler VTT okumaz). `toward`: TV'nin adresi; bu
/// bilgisayarın o TV'ye giden ağ adresi kullanılır (VPN ya da sanal ağ kartı varken önemli).
/// (medya adresi, VTT adresi) döner.
pub fn publish(
    body: Body,
    ext: &str,
    mime: &str,
    sub: Option<String>,
    toward: Option<IpAddr>,
) -> Result<(String, Option<String>), String> {
    let base = base_url(toward)?;
    let name = format!("media.{ext}");
    let mut files = HashMap::new();
    files.insert(
        name.clone(),
        Entry {
            body,
            mime: mime.into(),
        },
    );
    let sub_url = sub.map(|text| add_sub(&mut files, &base, text));
    *FILES.lock().unwrap() = Some(files);
    Ok((format!("{base}/{name}"), sub_url))
}

/// Yalnızca altyazıyı sunar (video doğrudan `https` akışken).
pub fn publish_sub(text: String) -> Result<String, String> {
    let base = base_url(None)?;
    let mut files = HashMap::new();
    let url = add_sub(&mut files, &base, text);
    *FILES.lock().unwrap() = Some(files);
    Ok(url)
}

fn add_sub(files: &mut HashMap<String, Entry>, base: &str, vtt: String) -> String {
    files.insert(
        "sub.srt".into(),
        Entry {
            body: Body::Text(vtt_to_srt(&vtt)),
            mime: "text/srt; charset=utf-8".into(),
        },
    );
    files.insert(
        "sub.vtt".into(),
        Entry {
            body: Body::Text(vtt),
            mime: "text/vtt; charset=utf-8".into(),
        },
    );
    format!("{base}/sub.vtt")
}

/// WebVTT → SRT: başlık atılır, zamanlarda nokta virgül olur, ipuçları numaralanır.
fn vtt_to_srt(vtt: &str) -> String {
    let mut out = String::new();
    let mut n = 0;
    let mut in_cue = false;
    for line in vtt.lines() {
        if line.contains("-->") {
            n += 1;
            in_cue = true;
            // VTT konum ayarları (`align:start` gibi) SRT'de yok.
            let mut parts = line.split_whitespace();
            let (a, b) = (parts.next().unwrap_or(""), parts.nth(1).unwrap_or(""));
            let fix = |t: &str| {
                let t = t.replace('.', ",");
                // VTT saatsiz olabilir (`01:02.500`).
                if t.matches(':').count() == 1 {
                    format!("00:{t}")
                } else {
                    t
                }
            };
            out.push_str(&format!("{n}\n{} --> {}\n", fix(a), fix(b)));
        } else if line.trim().is_empty() {
            if in_cue {
                out.push('\n');
            }
            in_cue = false;
        } else if in_cue {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Dosya adı ya da adresin uzantısı (küçük harf); yoksa boş.
pub fn ext(path: &str) -> String {
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .split(['?', '#'])
        .next()
        .unwrap_or("");
    match name.rsplit_once('.') {
        Some((_, e)) if !e.is_empty() && e.len() <= 5 => e.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Uzantıdan içerik türü; bilinmiyorsa `application/octet-stream`.
pub fn mime_of(ext: &str) -> &'static str {
    match ext {
        "mp4" | "m4v" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "ts" => "video/mp2t",
        "m3u8" => "application/vnd.apple.mpegurl",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

/// `http://<ağ adresi>:<port>/<belirteç>`; sunucu ilk çağrıda açılır.
fn base_url(toward: Option<IpAddr>) -> Result<String, String> {
    let ip = lan_ip(toward).ok_or_else(|| {
        trs(
            "bu bilgisayar bir ağa bağlı değil",
            "this computer isn't on a network",
        )
        .to_string()
    })?;
    let s = start()?;
    Ok(format!("http://{ip}:{}/{}", s.port, s.token))
}

fn start() -> Result<&'static Server, String> {
    let _g = START.lock().unwrap();
    if let Some(s) = SERVER.get() {
        return Ok(s);
    }
    let listener = TcpListener::bind("0.0.0.0:0").map_err(|e| {
        tr!(
            "ağ sunucusu açılamadı: {e}",
            "couldn't start the network server: {e}"
        )
    })?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    std::thread::Builder::new()
        .name("koni-cast-serve".into())
        .spawn(move || {
            for conn in listener.incoming().flatten() {
                let _ = std::thread::Builder::new()
                    .name("koni-cast-conn".into())
                    .spawn(move || handle(conn));
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(SERVER.get_or_init(|| Server {
        port,
        token: token(),
    }))
}

/// 128 bit rastgele belirteç (std'nin işletim sisteminden tohumlanan SipHash anahtarları).
fn token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut out = String::new();
    for i in 0..2u8 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(now);
        h.write_u8(i);
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Bu bilgisayarın `toward` adresine (yoksa varsayılan yola) giden ağ adresi. UDP
/// `connect` paket göndermez, yalnızca yol seçer (192.0.2.1 belgelere ayrılmış bir adrestir).
pub fn lan_ip(toward: Option<IpAddr>) -> Option<IpAddr> {
    let route = |to: IpAddr| -> Option<IpAddr> {
        let s = UdpSocket::bind("0.0.0.0:0").ok()?;
        s.connect((to, 9)).ok()?;
        let ip = s.local_addr().ok()?.ip();
        (!ip.is_unspecified() && !ip.is_loopback() && is_lan(ip)).then_some(ip)
    };
    toward
        .filter(|t| t.is_ipv4())
        .and_then(route)
        .or_else(|| route(IpAddr::from([192, 0, 2, 1])))
}

/// Ev ağı adresi mi (özel aralıklar, link-local, bu bilgisayar).
fn is_lan(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_private() || v.is_link_local() || v.is_loopback(),
        IpAddr::V6(v) => match v.to_ipv4_mapped() {
            Some(v4) => is_lan(IpAddr::V4(v4)),
            None => {
                let s = v.segments()[0];
                v.is_loopback() || (s & 0xfe00) == 0xfc00 || (s & 0xffc0) == 0xfe80
            }
        },
    }
}

fn handle(mut conn: TcpStream) {
    if !conn.peer_addr().is_ok_and(|a| is_lan(a.ip())) {
        return;
    }
    let _ = conn.set_read_timeout(Some(Duration::from_secs(10)));
    let Ok(reader) = conn.try_clone() else { return };
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let mut range = None;
    let mut caption = false;
    for _ in 0..64 {
        let mut h = String::new();
        if reader.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            let k = k.trim();
            if k.eq_ignore_ascii_case("range") {
                range = Some(v.trim().to_string());
            } else if k.eq_ignore_ascii_case("getcaptioninfo.sec") {
                caption = true;
            }
        }
    }
    let _ = respond(&mut conn, &method, &target, range.as_deref(), caption);
}

const CORS: &str = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\nAccess-Control-Allow-Methods: GET, HEAD, OPTIONS\r\n";

/// DLNA içerik özellikleri: baytla sarma var, dönüştürülmemiş, akış aktarımı.
pub const FEATURES: &str =
    "DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000";

fn status(conn: &mut TcpStream, code: &str) -> std::io::Result<()> {
    write!(
        conn,
        "HTTP/1.1 {code}\r\n{CORS}Content-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

fn respond(
    conn: &mut TcpStream,
    method: &str,
    target: &str,
    range: Option<&str>,
    caption: bool,
) -> std::io::Result<()> {
    if method == "OPTIONS" {
        return status(conn, "204 No Content");
    }
    if method != "GET" && method != "HEAD" {
        return status(conn, "405 Method Not Allowed");
    }
    let Some(s) = SERVER.get() else {
        return status(conn, "404 Not Found");
    };
    let path = target.split('?').next().unwrap_or("");
    let name = path
        .strip_prefix('/')
        .and_then(|p| p.strip_prefix(s.token.as_str()))
        .and_then(|p| p.strip_prefix('/'));
    let (entry, has_srt) = {
        let files = FILES.lock().unwrap();
        let files = files.as_ref();
        (
            name.and_then(|n| files?.get(n).map(|e| (e.body.clone(), e.mime.clone()))),
            files.is_some_and(|f| f.contains_key("sub.srt")),
        )
    };
    let Some((body, mime)) = entry else {
        return status(conn, "404 Not Found");
    };
    let head = method == "HEAD";
    // Samsung TV'ler altyazı adresini bu başlıkla sorar.
    // DLNA TV'lerin (özellikle Samsung) beklediği akış başlıkları; tarayıcılar yok sayar.
    let mut dlna =
        format!("transferMode.dlna.org: Streaming\r\ncontentFeatures.dlna.org: {FEATURES}\r\n");
    if caption && has_srt {
        let host = conn.local_addr()?;
        dlna.push_str(&format!(
            "CaptionInfo.sec: http://{host}/{}/sub.srt\r\n",
            s.token
        ));
    }
    match body {
        Body::Text(t) => {
            write!(
                conn,
                "HTTP/1.1 200 OK\r\n{CORS}Content-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                t.len()
            )?;
            if !head {
                conn.write_all(t.as_bytes())?;
            }
            Ok(())
        }
        Body::Remote { url, referer } => {
            relay(conn, &url, referer.as_deref(), &mime, range, head, &dlna)
        }
        Body::File(p) => {
            let Ok(mut f) = File::open(&p) else {
                return status(conn, "404 Not Found");
            };
            let size = f.metadata()?.len();
            let (start, end) = match range.and_then(|r| parse_range(r, size)) {
                Some(r) => r,
                None if range.is_some() && size > 0 => {
                    return write!(
                        conn,
                        "HTTP/1.1 416 Range Not Satisfiable\r\n{CORS}Content-Range: bytes */{size}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                }
                None => (0, size.saturating_sub(1)),
            };
            let len = if size == 0 { 0 } else { end - start + 1 };
            let partial = range.is_some() && size > 0;
            let first = if partial {
                format!("206 Partial Content\r\nContent-Range: bytes {start}-{end}/{size}")
            } else {
                "200 OK".into()
            };
            write!(
                conn,
                "HTTP/1.1 {first}\r\n{CORS}Content-Type: {mime}\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\n{dlna}Connection: close\r\n\r\n"
            )?;
            if head || len == 0 {
                return Ok(());
            }
            f.seek(SeekFrom::Start(start))?;
            std::io::copy(&mut f.take(len), conn)?;
            Ok(())
        }
    }
}

/// Uzak akış istemcisi: toplam süre sınırı yok (film saatlerce akar), yalnızca bağlantı
/// ve okuma beklemesi sınırlı.
fn remote_client() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        crate::addons::ensure_tls();
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .user_agent(crate::media::BROWSER_UA)
            .build()
            .expect("HTTP istemcisi kurulamadı")
    });
    &CLIENT
}

/// Uzak akışı TV'ye aktarır; `Range` isteği olduğu gibi kaynağa iletilir.
fn relay(
    conn: &mut TcpStream,
    url: &str,
    referer: Option<&str>,
    mime: &str,
    range: Option<&str>,
    head: bool,
    dlna: &str,
) -> std::io::Result<()> {
    use reqwest::header::{CONTENT_RANGE, RANGE, REFERER};
    use tauri::async_runtime::block_on;
    let mut req = remote_client().get(url);
    if let Some(r) = referer {
        req = req.header(REFERER, r);
    }
    if let Some(r) = range {
        req = req.header(RANGE, r);
    }
    let mut res = match block_on(req.send()) {
        Ok(r) => r,
        Err(e) => {
            super::log(&format!(
                "TV aktarımı: kaynak açılamadı: {}",
                e.without_url()
            ));
            return status(conn, "502 Bad Gateway");
        }
    };
    let code = res.status().as_u16();
    let content_range = res
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let first = match (code, content_range) {
        (200, _) => "200 OK".to_string(),
        (206, Some(cr)) => format!("206 Partial Content\r\nContent-Range: {cr}"),
        (416, cr) => {
            let cr = cr
                .map(|c| format!("Content-Range: {c}\r\n"))
                .unwrap_or_default();
            return write!(
                conn,
                "HTTP/1.1 416 Range Not Satisfiable\r\n{CORS}{cr}Content-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
        _ => {
            super::log(&format!("TV aktarımı: kaynak {code} döndü"));
            return status(conn, "502 Bad Gateway");
        }
    };
    let len = res
        .content_length()
        .map(|l| format!("Content-Length: {l}\r\n"))
        .unwrap_or_default();
    write!(
        conn,
        "HTTP/1.1 {first}\r\n{CORS}Content-Type: {mime}\r\n{len}Accept-Ranges: bytes\r\n{dlna}Connection: close\r\n\r\n"
    )?;
    if head {
        return Ok(());
    }
    // TV sararken bağlantıyı keser: yazma hatası olur, kaynağa giden istek de kapanır.
    while let Some(chunk) = block_on(res.chunk()).map_err(std::io::Error::other)? {
        conn.write_all(&chunk)?;
    }
    Ok(())
}

/// `bytes=a-b`, `bytes=a-`, `bytes=-n` → kapalı aralık; geçersizse `None`.
fn parse_range(r: &str, size: u64) -> Option<(u64, u64)> {
    let spec = r.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    if size == 0 {
        return None;
    }
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (size.saturating_sub(n), size - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() {
            size - 1
        } else {
            b.parse::<u64>().ok()?.min(size - 1)
        };
        (start, end)
    };
    (start <= end && start < size).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=10-19", 100), Some((10, 19)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=50-500", 100), Some((50, 99)));
        assert_eq!(parse_range("bytes=200-", 100), None);
    }

    #[test]
    fn extensions() {
        assert_eq!(ext("C:\\Filmler\\Film.Adı.MKV"), "mkv");
        assert_eq!(ext("https://x.example/a/b/film.mp4?token=1.2"), "mp4");
        assert_eq!(ext("https://x.example/dl/abc123"), "");
        assert_eq!(mime_of(&ext("/home/a/film.webm")), "video/webm");
    }

    #[test]
    fn srt_from_vtt() {
        let vtt = "WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.500 align:start\nMerhaba\n\n02:03.000 --> 02:04.000\nİki\nsatır\n";
        assert_eq!(
            vtt_to_srt(vtt),
            "1\n00:00:01,000 --> 00:00:02,500\nMerhaba\n\n2\n00:02:03,000 --> 00:02:04,000\nİki\nsatır\n"
        );
    }

    #[test]
    fn lan_only() {
        assert!(is_lan("192.168.1.20".parse().unwrap()));
        assert!(is_lan("10.0.0.5".parse().unwrap()));
        assert!(is_lan("fe80::1".parse().unwrap()));
        assert!(!is_lan("203.0.113.5".parse().unwrap()));
        assert!(!is_lan("2001:db8::1".parse().unwrap()));
    }
}
