//! Ev ağına açık küçük HTTP sunucu: TV, oynatılan yerel dosyayı ve altyazıyı buradan okur.
//!
//! İlk gönderimde açılır, uygulama kapanınca kapanır. Yalnızca yerel ağ adreslerinden
//! gelen isteklere cevap verir; yol, uygulama her açıldığında değişen rastgele bir
//! belirteç taşır. Aynı anda yalnızca son hazırlanan medya sunulur.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{IpAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::i18n::trs;

#[derive(Clone)]
enum Body {
    File(PathBuf),
    Text(String),
}

struct Entry {
    body: Body,
    mime: &'static str,
}

struct Server {
    port: u16,
    token: String,
}

static SERVER: OnceLock<Server> = OnceLock::new();
static START: Mutex<()> = Mutex::new(());
/// Sunulan dosyalar: ad (`media.mp4`, `sub.vtt`) → içerik.
static FILES: Mutex<Option<HashMap<String, Entry>>> = Mutex::new(None);

/// Yerel dosyayı (ve VTT altyazıyı) sunar; önceki medya kalkar. Adresleri döndürür.
pub fn publish(path: &str, sub: Option<String>) -> Result<(String, Option<String>), String> {
    let base = base_url()?;
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_ascii_lowercase();
    let name = format!("media.{ext}");
    let mut files = HashMap::new();
    files.insert(
        name.clone(),
        Entry {
            body: Body::File(path.into()),
            mime: mime(path),
        },
    );
    let sub_url = sub.map(|text| {
        files.insert("sub.vtt".into(), sub_entry(text));
        format!("{base}/sub.vtt")
    });
    *FILES.lock().unwrap() = Some(files);
    Ok((format!("{base}/{name}"), sub_url))
}

/// Yalnızca altyazıyı sunar (video doğrudan `https` akışken).
pub fn publish_sub(text: String) -> Result<String, String> {
    let base = base_url()?;
    let mut files = HashMap::new();
    files.insert("sub.vtt".into(), sub_entry(text));
    *FILES.lock().unwrap() = Some(files);
    Ok(format!("{base}/sub.vtt"))
}

fn sub_entry(text: String) -> Entry {
    Entry {
        body: Body::Text(text),
        mime: "text/vtt; charset=utf-8",
    }
}

/// Dosya uzantısından içerik türü.
pub fn mime(path: &str) -> &'static str {
    let ext = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .split('?')
        .next()
        .unwrap_or("")
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
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
fn base_url() -> Result<String, String> {
    let ip = lan_ip().ok_or_else(|| {
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

/// Varsayılan ağ arabiriminin adresi. UDP `connect` paket göndermez, yalnızca yol seçer
/// (192.0.2.1 belgelere ayrılmış bir adrestir).
fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified() && is_lan(ip)).then_some(ip)
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
    for _ in 0..64 {
        let mut h = String::new();
        if reader.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("range") {
                range = Some(v.trim().to_string());
            }
        }
    }
    let _ = respond(&mut conn, &method, &target, range.as_deref());
}

const CORS: &str = "Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\nAccess-Control-Allow-Methods: GET, HEAD, OPTIONS\r\n";

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
    let entry = name.and_then(|n| {
        let files = FILES.lock().unwrap();
        files.as_ref()?.get(n).map(|e| (e.body.clone(), e.mime))
    });
    let Some((body, mime)) = entry else {
        return status(conn, "404 Not Found");
    };
    let head = method == "HEAD";
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
                "HTTP/1.1 {first}\r\n{CORS}Content-Type: {mime}\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n\r\n"
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
    fn lan_only() {
        assert!(is_lan("192.168.1.20".parse().unwrap()));
        assert!(is_lan("10.0.0.5".parse().unwrap()));
        assert!(is_lan("fe80::1".parse().unwrap()));
        assert!(!is_lan("203.0.113.5".parse().unwrap()));
        assert!(!is_lan("2001:db8::1".parse().unwrap()));
    }
}
