//! Chromecast: mDNS ile `_googlecast._tcp` keşfi, Cast v2 ile Default Media Receiver.
//!
//! Bağlantı TLS üstünden (8009; cihazın sertifikası kendinden imzalı, doğrulanmaz).
//! Her mesaj 4 bayt uzunluk + protobuf `CastMessage`. Tek bağlantı tutulur: her komut
//! isteğini yazar, aynı `requestId`'li yanıtı okur, araya giren PING'e PONG döner. TV
//! modunda arayüz durumu saniyede bir sorduğu için bağlantı canlı kalır; kopmuşsa bir
//! kez yeniden bağlanılır.

use super::{Control, Device, Media, Position};
use crate::i18n::trs;
use mdns_sd::{ServiceDaemon, ServiceEvent};
use native_tls::{TlsConnector, TlsStream};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

pub const KIND: &str = "chromecast";
const SERVICE: &str = "_googlecast._tcp.local.";
/// Default Media Receiver.
const APP: &str = "CC1AD845";
const NS_CONN: &str = "urn:x-cast:com.google.cast.tp.connection";
const NS_BEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
const NS_RECV: &str = "urn:x-cast:com.google.cast.receiver";
const NS_MEDIA: &str = "urn:x-cast:com.google.cast.media";
const SENDER: &str = "sender-koni";
const RECEIVER: &str = "receiver-0";

/// Bulunan cihazlar (mDNS tam adı → cihaz). Keşif ilk çağrıda başlar, uygulama boyunca sürer.
static FOUND: Mutex<BTreeMap<String, Device>> = Mutex::new(BTreeMap::new());
static BROWSE: Once = Once::new();

pub fn devices() -> Vec<Device> {
    BROWSE.call_once(|| {
        if let Err(e) = browse() {
            eprintln!("[koni] Chromecast keşfi başlamadı: {e}");
        }
    });
    lock(&FOUND).values().cloned().collect()
}

fn browse() -> Result<(), mdns_sd::Error> {
    let mdns = ServiceDaemon::new()?;
    let rx = mdns.browse(SERVICE)?;
    std::thread::spawn(move || {
        let _mdns = mdns;
        while let Ok(ev) = rx.recv() {
            match ev {
                ServiceEvent::ServiceResolved(s) => {
                    let Some(ip) = s.get_addresses_v4().into_iter().next() else {
                        continue;
                    };
                    let name = s
                        .get_property_val_str("fn")
                        .unwrap_or(s.host.trim_end_matches(".local."))
                        .to_string();
                    let id = format!("{ip}:{}", s.get_port());
                    let dev = Device {
                        id,
                        name,
                        kind: KIND.into(),
                    };
                    lock(&FOUND).insert(s.get_fullname().to_string(), dev);
                }
                ServiceEvent::ServiceRemoved(_, full) => {
                    lock(&FOUND).remove(&full);
                }
                _ => {}
            }
        }
    });
    Ok(())
}

/// `media`yı TV'de açar (altyazı varsa açık gelir).
pub fn play(device: &Device, media: &Media) -> Result<(), String> {
    with_conn(&device.id, |c| {
        let (tid, sid) = c.app(true)?.ok_or_else(failed)?;
        let mut m = json!({
            "contentId": media.url,
            "contentType": media.mime,
            "streamType": "BUFFERED",
            "metadata": { "metadataType": 0, "title": media.title },
        });
        let mut load = json!({ "type": "LOAD", "sessionId": sid, "autoplay": true });
        if let Some(sub) = &media.sub_url {
            m["tracks"] = json!([{
                "trackId": 1, "type": "TEXT", "subtype": "SUBTITLES",
                "trackContentId": sub, "trackContentType": "text/vtt", "name": trs("Altyazı", "Subtitles"),
            }]);
            load["activeTrackIds"] = json!([1]);
        }
        load["media"] = m;
        let r = c.request(&tid, NS_MEDIA, load)?;
        if r["type"] != "MEDIA_STATUS" {
            return Err(failed());
        }
        Ok(())
    })
}

/// TV'deki konum; uygulama kapalıysa ya da video bittiyse sıfır.
pub fn position(device: &Device) -> Result<Position, String> {
    with_conn(&device.id, |c| {
        let idle = Position {
            position: 0.0,
            duration: 0.0,
            playing: false,
        };
        let Some((tid, _)) = c.app(false)? else {
            return Ok(idle);
        };
        let r = c.request(&tid, NS_MEDIA, json!({ "type": "GET_STATUS" }))?;
        let s = &r["status"][0];
        if s.is_null() {
            return Ok(idle);
        }
        Ok(Position {
            position: s["currentTime"].as_f64().unwrap_or(0.0),
            duration: s["media"]["duration"].as_f64().unwrap_or(0.0),
            playing: matches!(s["playerState"].as_str(), Some("PLAYING" | "BUFFERING")),
        })
    })
}

/// Oynat / duraklat / sar; durdur TV'deki uygulamayı kapatır.
pub fn control(device: &Device, ctl: Control) -> Result<(), String> {
    with_conn(&device.id, |c| {
        let Some((tid, sid)) = c.app(false)? else {
            return Ok(());
        };
        let typ = match ctl {
            Control::Stop => {
                c.request(
                    RECEIVER,
                    NS_RECV,
                    json!({ "type": "STOP", "sessionId": sid }),
                )?;
                c.transport = None;
                return Ok(());
            }
            Control::Play => "PLAY",
            Control::Pause => "PAUSE",
            Control::Seek(_) => "SEEK",
        };
        let r = c.request(&tid, NS_MEDIA, json!({ "type": "GET_STATUS" }))?;
        let Some(msid) = r["status"][0]["mediaSessionId"].as_i64() else {
            return Ok(());
        };
        let mut cmd = json!({ "type": typ, "mediaSessionId": msid });
        if let Control::Seek(t) = ctl {
            cmd["currentTime"] = t.max(0.0).into();
        }
        c.request(&tid, NS_MEDIA, cmd).map(|_| ())
    })
}

fn failed() -> String {
    trs("TV videoyu açamadı", "the TV couldn't open the video").into()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------- bağlantı

static CONN: Mutex<Option<Conn>> = Mutex::new(None);

/// `addr` cihazına açık bağlantıyla `f`'i çalıştırır; eski bağlantı kopmuşsa bir kez
/// yeniden bağlanıp tekrar dener.
fn with_conn<T>(addr: &str, f: impl Fn(&mut Conn) -> Result<T, String>) -> Result<T, String> {
    let mut slot = lock(&CONN);
    if let Some(c) = slot.as_mut().filter(|c| c.addr == addr) {
        if let Ok(v) = f(c) {
            return Ok(v);
        }
    }
    *slot = None;
    let mut c = Conn::open(addr)?;
    let v = f(&mut c);
    if v.is_ok() {
        *slot = Some(c);
    }
    v
}

struct Conn {
    addr: String,
    tls: TlsStream<TcpStream>,
    next_id: u64,
    /// Sanal bağlantı açılmış uygulama (`transportId`).
    transport: Option<String>,
}

impl Conn {
    fn open(addr: &str) -> Result<Conn, String> {
        let err = |e: &dyn std::fmt::Display| format!("Chromecast: {e}");
        let sock: SocketAddr = addr.parse().map_err(|e| err(&e))?;
        let tcp = TcpStream::connect_timeout(&sock, Duration::from_secs(5)).map_err(|e| err(&e))?;
        tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
        tcp.set_write_timeout(Some(Duration::from_secs(5))).ok();
        let tls = TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()
            .map_err(|e| err(&e))?
            .connect(&sock.ip().to_string(), tcp)
            .map_err(|e| err(&e))?;
        let mut c = Conn {
            addr: addr.into(),
            tls,
            next_id: 1,
            transport: None,
        };
        c.send(
            RECEIVER,
            NS_CONN,
            &json!({ "type": "CONNECT", "origin": {} }),
        )?;
        Ok(c)
    }

    /// Default Media Receiver'ın `(transportId, sessionId)`'si; `launch` ise gerekirse açar.
    fn app(&mut self, launch: bool) -> Result<Option<(String, String)>, String> {
        let mut r = self.request(RECEIVER, NS_RECV, json!({ "type": "GET_STATUS" }))?;
        if find_app(&r).is_none() && launch {
            r = self.request(RECEIVER, NS_RECV, json!({ "type": "LAUNCH", "appId": APP }))?;
        }
        let Some((tid, sid)) = find_app(&r) else {
            return Ok(None);
        };
        if self.transport.as_deref() != Some(tid.as_str()) {
            self.send(&tid, NS_CONN, &json!({ "type": "CONNECT", "origin": {} }))?;
            self.transport = Some(tid.clone());
        }
        Ok(Some((tid, sid)))
    }

    fn send(&mut self, dest: &str, ns: &str, payload: &Value) -> Result<(), String> {
        let body = encode(SENDER, dest, ns, &payload.to_string());
        let mut frame = (body.len() as u32).to_be_bytes().to_vec();
        frame.extend(body);
        self.tls
            .write_all(&frame)
            .map_err(|e| format!("Chromecast: {e}"))
    }

    /// İsteği gönderir, aynı `requestId`'li yanıtı bekler.
    fn request(&mut self, dest: &str, ns: &str, mut payload: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        payload["requestId"] = id.into();
        self.send(dest, ns, &payload)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let (ns_in, data) = self.read()?;
            let v: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
            if ns_in == NS_BEAT && v["type"] == "PING" {
                self.send(RECEIVER, NS_BEAT, &json!({ "type": "PONG" }))?;
            } else if v["requestId"] == id {
                return match v["type"].as_str() {
                    Some("LOAD_FAILED" | "LAUNCH_ERROR" | "INVALID_REQUEST") => Err(failed()),
                    _ => Ok(v),
                };
            }
        }
        Err("Chromecast: zaman aşımı".into())
    }

    /// Bir mesaj okur: `(namespace, payload)`.
    fn read(&mut self) -> Result<(String, String), String> {
        let err = |e: std::io::Error| format!("Chromecast: {e}");
        let mut len = [0u8; 4];
        self.tls.read_exact(&mut len).map_err(err)?;
        let len = u32::from_be_bytes(len) as usize;
        if len > 1 << 20 {
            return Err("Chromecast: bozuk mesaj".into());
        }
        let mut body = vec![0u8; len];
        self.tls.read_exact(&mut body).map_err(err)?;
        decode(&body).ok_or_else(|| "Chromecast: bozuk mesaj".into())
    }
}

fn find_app(r: &Value) -> Option<(String, String)> {
    let a = r["status"]["applications"]
        .as_array()?
        .iter()
        .find(|a| a["appId"] == APP)?;
    Some((
        a["transportId"].as_str()?.into(),
        a["sessionId"].as_str()?.into(),
    ))
}

// ---------------------------------------------------------------- CastMessage (protobuf)
// 1 protocol_version (0 = CASTV2_1_0), 2 source_id, 3 destination_id, 4 namespace,
// 5 payload_type (0 = STRING), 6 payload_utf8.

fn encode(src: &str, dest: &str, ns: &str, data: &str) -> Vec<u8> {
    let mut b = vec![0x08, 0x00];
    for (n, s) in [(2u8, src), (3, dest), (4, ns)] {
        string_field(&mut b, n, s);
    }
    b.extend([0x28, 0x00]);
    string_field(&mut b, 6, data);
    b
}

fn string_field(b: &mut Vec<u8>, n: u8, s: &str) {
    b.push((n << 3) | 2);
    let mut v = s.len();
    while v >= 0x80 {
        b.push(v as u8 | 0x80);
        v >>= 7;
    }
    b.push(v as u8);
    b.extend_from_slice(s.as_bytes());
}

/// `(namespace, payload_utf8)`; diğer alanlar atlanır.
fn decode(mut b: &[u8]) -> Option<(String, String)> {
    fn varint(b: &mut &[u8]) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let (&x, rest) = b.split_first()?;
            *b = rest;
            v |= u64::from(x & 0x7f) << shift;
            if x & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }
    let (mut ns, mut data) = (String::new(), String::new());
    while !b.is_empty() {
        let key = varint(&mut b)?;
        match key & 7 {
            0 => {
                varint(&mut b)?;
            }
            2 => {
                let len = usize::try_from(varint(&mut b)?).ok()?;
                if len > b.len() {
                    return None;
                }
                let (s, rest) = b.split_at(len);
                b = rest;
                match key >> 3 {
                    4 => ns = String::from_utf8_lossy(s).into_owned(),
                    6 => data = String::from_utf8_lossy(s).into_owned(),
                    _ => {}
                }
            }
            _ => return None,
        }
    }
    Some((ns, data))
}

#[cfg(test)]
mod tests {
    #[test]
    fn cast_message_roundtrip() {
        let data = "x".repeat(300);
        let b = super::encode("sender-koni", "receiver-0", super::NS_MEDIA, &data);
        assert_eq!(super::decode(&b), Some((super::NS_MEDIA.to_string(), data)));
    }
}
