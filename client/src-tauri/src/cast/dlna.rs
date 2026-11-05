//! DLNA / UPnP TV'ler (Samsung, LG, Sony…): SSDP ile keşif, AVTransport SOAP ile kontrol.
//!
//! Ağır bağımlılık yok: SSDP için UDP, açıklama belgesi ve SOAP için düz HTTP/1.1
//! (yalnız ev ağı, TLS yok). XML'den yalnızca gereken birkaç etiket okunur.

use super::{Control, Device, Media, Position};
use crate::i18n::trs;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const AVT: &str = "urn:schemas-upnp-org:service:AVTransport:1";
/// Son aramada yanıt vermese de bu kadar süre listede kalır (bazı TV'ler her aramaya yanıt vermez).
const KEEP: Duration = Duration::from_secs(15);

struct Renderer {
    name: String,
    /// AVTransport `controlURL`, tam adres.
    control: String,
    seen: Instant,
}

/// Bulunan TV'ler, `Device.id` ile. Açıklama belgesi yer (LOCATION) başına bir kez okunur.
static FOUND: Mutex<Option<HashMap<String, Renderer>>> = Mutex::new(None);
static BY_LOCATION: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Ağdaki DLNA oynatıcılar (~1,5 s sürer).
pub fn devices() -> Vec<Device> {
    let now = Instant::now();
    for loc in search() {
        let known = BY_LOCATION
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|m| m.get(&loc).cloned());
        let id = match known {
            Some(id) => id,
            None => {
                let Some((id, r)) = describe(&loc) else {
                    continue;
                };
                BY_LOCATION
                    .lock()
                    .unwrap()
                    .get_or_insert_with(HashMap::new)
                    .insert(loc, id.clone());
                FOUND
                    .lock()
                    .unwrap()
                    .get_or_insert_with(HashMap::new)
                    .insert(id.clone(), r);
                id
            }
        };
        if let Some(r) = FOUND.lock().unwrap().as_mut().and_then(|m| m.get_mut(&id)) {
            r.seen = now;
        }
    }
    let found = FOUND.lock().unwrap();
    let mut out: Vec<Device> = found
        .iter()
        .flatten()
        .filter(|(_, r)| now.duration_since(r.seen) < KEEP)
        .map(|(id, r)| Device {
            id: id.clone(),
            name: r.name.clone(),
            kind: "dlna".into(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// SSDP `M-SEARCH`: yanıt veren cihazların açıklama belgesi adresleri.
fn search() -> Vec<String> {
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else {
        return Vec::new();
    };
    let _ = sock.set_read_timeout(Some(Duration::from_millis(300)));
    for st in [AVT, "urn:schemas-upnp-org:device:MediaRenderer:1"] {
        let msg = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: {st}\r\n\r\n"
        );
        let _ = sock.send_to(msg.as_bytes(), "239.255.255.250:1900");
    }
    let end = Instant::now() + Duration::from_millis(1500);
    let mut out = Vec::new();
    let mut buf = [0u8; 2048];
    while Instant::now() < end {
        let Ok((n, _)) = sock.recv_from(&mut buf) else {
            // Windows'ta UDP hatası hemen dönebilir; boşa dönmesin.
            std::thread::sleep(Duration::from_millis(20));
            continue;
        };
        let text = String::from_utf8_lossy(&buf[..n]);
        if let Some(loc) = header(&text, "location") {
            if loc.starts_with("http://") && !out.iter().any(|l| l == loc) {
                out.push(loc.to_string());
            }
        }
    }
    out
}

/// Açıklama belgesinden ad ve AVTransport kontrol adresi; AVTransport yoksa `None`.
fn describe(location: &str) -> Option<(String, Renderer)> {
    let (status, xml) = http(location, "GET", &[], "").ok()?;
    if status != 200 {
        return None;
    }
    let control = xml
        .split("<service>")
        .find(|s| s.contains(AVT))
        .and_then(|s| tag(s, "controlURL"))?;
    let base = tag(&xml, "URLBase").unwrap_or(location);
    let name = tag(&xml, "friendlyName")
        .map(unescape)
        .unwrap_or_else(|| "TV".into());
    let udn = tag(&xml, "UDN").unwrap_or(location);
    Some((
        format!("dlna:{udn}"),
        Renderer {
            name,
            control: join(base, control),
            seen: Instant::now(),
        },
    ))
}

fn control_url(device: &Device) -> Result<String, String> {
    FOUND
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(&device.id))
        .map(|r| r.control.clone())
        .ok_or_else(|| format!("{}: {}", trs("TV bulunamadı", "TV not found"), device.name))
}

/// `media`yı TV'de açar ve oynamaya başladığını bekler.
pub fn play(device: &Device, media: &Media) -> Result<(), String> {
    let url = control_url(device)?;
    let _ = soap(&url, "Stop", "");
    let meta = didl(media);
    let set = soap(
        &url,
        "SetAVTransportURI",
        &format!(
            "<CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
            escape(&media.url),
            escape(&meta)
        ),
    );
    if let Err(e) = set {
        return Err(cant_open(e));
    }
    let _ = soap(&url, "Play", "<Speed>1</Speed>");
    // Açılıp açılmadığını durumdan anlarız; bazı TV'ler kendiliğinden oynatır, bazıları
    // `Play` hatası verip yine de oynatır.
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let info = soap(&url, "GetTransportInfo", "").unwrap_or_default();
        let state = tag(&info, "CurrentTransportState").unwrap_or("");
        let status = tag(&info, "CurrentTransportStatus").unwrap_or("OK");
        if status == "ERROR_OCCURRED" {
            return Err(cant_open(String::new()));
        }
        if state == "PLAYING" {
            return Ok(());
        }
        if Instant::now() >= end {
            return match state {
                "STOPPED" | "NO_MEDIA_PRESENT" => Err(cant_open(String::new())),
                _ => Ok(()), // hâlâ yükleniyor
            };
        }
    }
}

fn cant_open(detail: String) -> String {
    let msg = trs(
        "TV bu dosyayı açamadı (biçim desteklenmiyor olabilir)",
        "The TV couldn't open this file (the format may not be supported)",
    );
    if detail.is_empty() {
        msg.into()
    } else {
        format!("{msg}: {detail}")
    }
}

pub fn control(device: &Device, c: Control) -> Result<(), String> {
    let url = control_url(device)?;
    match c {
        Control::Play => soap(&url, "Play", "<Speed>1</Speed>"),
        Control::Pause => soap(&url, "Pause", ""),
        Control::Stop => soap(&url, "Stop", ""),
        Control::Seek(s) => soap(
            &url,
            "Seek",
            &format!("<Unit>REL_TIME</Unit><Target>{}</Target>", hms(s)),
        ),
    }
    .map(|_| ())
}

pub fn position(device: &Device) -> Result<Position, String> {
    let url = control_url(device)?;
    let pos = soap(&url, "GetPositionInfo", "")?;
    let info = soap(&url, "GetTransportInfo", "").unwrap_or_default();
    Ok(Position {
        position: tag(&pos, "RelTime").and_then(secs).unwrap_or(0.0),
        duration: tag(&pos, "TrackDuration").and_then(secs).unwrap_or(0.0),
        playing: tag(&info, "CurrentTransportState") == Some("PLAYING"),
    })
}

/// AVTransport eylemi; yanıt gövdesi ya da UPnP hata kodu.
fn soap(url: &str, action: &str, args: &str) -> Result<String, String> {
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\">\
<s:Body><u:{action} xmlns:u=\"{AVT}\"><InstanceID>0</InstanceID>{args}</u:{action}></s:Body></s:Envelope>"
    );
    let soap_action = format!("\"{AVT}#{action}\"");
    let headers = [
        ("Content-Type", "text/xml; charset=\"utf-8\""),
        ("SOAPACTION", soap_action.as_str()),
    ];
    let (status, resp) = http(url, "POST", &headers, &body)?;
    if status == 200 {
        return Ok(resp);
    }
    let code = tag(&resp, "errorCode").unwrap_or("");
    let desc = tag(&resp, "errorDescription").unwrap_or("");
    Err(format!("{action} {status} {code} {desc}")
        .trim()
        .to_string())
}

/// Tek seferlik düz HTTP isteği: (durum kodu, gövde).
fn http(
    url: &str,
    method: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Result<(u16, String), String> {
    let rest = url.strip_prefix("http://").ok_or("http")?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = if host.contains(':') {
        host.to_string()
    } else {
        format!("{host}:80")
    };
    let addr = addr
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("adres")?;
    let mut conn =
        TcpStream::connect_timeout(&addr, Duration::from_secs(3)).map_err(|e| e.to_string())?;
    let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = conn.set_write_timeout(Some(Duration::from_secs(5)));
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    conn.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    let _ = conn.take(1 << 20).read_to_end(&mut raw);
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("yanıt yok")?;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let rest = &raw[split + 4..];
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or("yanıt yok")?;
    let chunked =
        header(&head, "transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked"));
    let body = if chunked {
        dechunk(rest)
    } else {
        rest.to_vec()
    };
    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}

fn dechunk(mut s: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(i) = s.windows(2).position(|w| w == b"\r\n") {
        let size = String::from_utf8_lossy(&s[..i]);
        let n = usize::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        let rest = &s[i + 2..];
        let n = n.min(rest.len());
        out.extend_from_slice(&rest[..n]);
        if n == 0 {
            break;
        }
        s = rest[n..].strip_prefix(b"\r\n").unwrap_or(&rest[n..]);
    }
    out
}

/// Başlık değeri (büyük/küçük harf duyarsız).
fn header<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
    })
}

/// İlk `<name>…</name>` içeriği; ad alanı önekli etiketleri de (`<u:name>`) bulur.
fn tag<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = xml[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let before = xml[..at].chars().next_back();
        let after = xml[from..].chars().next();
        if !matches!(before, Some('<') | Some(':')) || !matches!(after, Some('>') | Some(' ')) {
            continue;
        }
        let open_start = xml[..at].rfind('<')?;
        if xml[open_start..at].contains('/') {
            continue; // kapanış etiketi
        }
        let start = from + xml[from..].find('>')? + 1;
        let end = start + xml[start..].find("</")?;
        return Some(xml[start..end].trim());
    }
    None
}

/// Göreli `controlURL`'yi tam adrese çevirir.
fn join(base: &str, url: &str) -> String {
    if url.starts_with("http://") {
        return url.to_string();
    }
    let rest = base.strip_prefix("http://").unwrap_or(base);
    let origin = &base[..base.len() - rest.len() + rest.find('/').unwrap_or(rest.len())];
    if url.starts_with('/') {
        format!("{origin}{url}")
    } else {
        format!("{origin}/{url}")
    }
}

/// TV'ye giden içerik bilgisi (DIDL-Lite). Samsung harici altyazıyı `sec:CaptionInfoEx` ile alır.
fn didl(m: &Media) -> String {
    let sub = m
        .sub_url
        .as_ref()
        .map(|s| {
            format!(
                "<sec:CaptionInfoEx sec:type=\"vtt\">{0}</sec:CaptionInfoEx>\
<res protocolInfo=\"http-get:*:text/vtt:*\">{0}</res>",
                escape(s)
            )
        })
        .unwrap_or_default();
    format!(
        "<DIDL-Lite xmlns=\"urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/\" \
xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:upnp=\"urn:schemas-upnp-org:metadata-1-0/upnp/\" \
xmlns:sec=\"http://www.sec.co.kr/\"><item id=\"0\" parentID=\"-1\" restricted=\"1\">\
<dc:title>{}</dc:title><upnp:class>object.item.videoItem</upnp:class>\
<res protocolInfo=\"http-get:*:{}:*\">{}</res>{sub}</item></DIDL-Lite>",
        escape(&m.title),
        m.mime,
        escape(&m.url)
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Saniye → `H:MM:SS`.
fn hms(s: f64) -> String {
    let t = s.max(0.0) as u64;
    format!("{}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
}

/// `H:MM:SS(.fff)` → saniye; `NOT_IMPLEMENTED` gibi değerler `None`.
fn secs(s: &str) -> Option<f64> {
    let mut parts = s.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_description() {
        let xml = "<root><URLBase>http://192.168.1.5:9197/</URLBase><device>\
<friendlyName>[TV] Salon &amp; Oda</friendlyName><UDN>uuid:abc</UDN><serviceList>\
<service><serviceType>urn:schemas-upnp-org:service:ConnectionManager:1</serviceType>\
<controlURL>/cm</controlURL></service>\
<service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType>\
<controlURL>upnp/control/AVTransport1</controlURL></service></serviceList></device></root>";
        let control = xml
            .split("<service>")
            .find(|s| s.contains(AVT))
            .and_then(|s| tag(s, "controlURL"))
            .unwrap();
        assert_eq!(
            join(tag(xml, "URLBase").unwrap(), control),
            "http://192.168.1.5:9197/upnp/control/AVTransport1"
        );
        assert_eq!(
            unescape(tag(xml, "friendlyName").unwrap()),
            "[TV] Salon & Oda"
        );
        assert_eq!(
            join("http://10.0.0.2:1400/xml/desc.xml", "/ctl"),
            "http://10.0.0.2:1400/ctl"
        );
    }

    #[test]
    fn reads_soap_reply() {
        let r = "<s:Envelope><s:Body><u:GetPositionInfoResponse xmlns:u=\"x\"><Track>1</Track>\
<TrackDuration>1:02:03</TrackDuration><RelTime>0:00:42.500</RelTime></u:GetPositionInfoResponse></s:Body></s:Envelope>";
        assert_eq!(tag(r, "RelTime").and_then(secs), Some(42.5));
        assert_eq!(tag(r, "TrackDuration").and_then(secs), Some(3723.0));
        assert_eq!(secs("NOT_IMPLEMENTED"), None);
        assert_eq!(hms(3723.9), "1:02:03");
        let e =
            "<s:Fault><detail><UPnPError><errorCode>714</errorCode></UPnPError></detail></s:Fault>";
        assert_eq!(tag(e, "errorCode"), Some("714"));
    }

    #[test]
    fn ssdp_and_chunked() {
        let r = "HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=1800\r\nLocation: http://192.168.1.5:9197/dmr\r\n\r\n";
        assert_eq!(header(r, "location"), Some("http://192.168.1.5:9197/dmr"));
        assert_eq!(
            dechunk(b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"),
            b"hello world"
        );
    }
}
