//! WebSocket mesajları. Sözleşme: docs/PROTOCOL.md (p0.8).
//!
//! Zamanlar sunucu duvar saati, Unix epoch ms (`i64`). Konumlar saniye (`f64`).
//! Bilinmeyen alanlar yok sayılır, bilinmeyen `t` değeri `ServerMsg::Unknown` olur.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- İstemci → Sunucu

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    CreateRoom {
        name: String,
        username: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        policy: Option<Policy>,
        /// Hesap oturum belirteci (hesaplı sunucuda zorunlu; ad hesaptan gelir).
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<String>,
    },
    Join {
        invite: String,
        username: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<String>,
    },
    /// Bağlantı koptuktan sonra aynı üyeyle dönüş (`joined.resume` belirteciyle).
    Resume {
        room: String,
        member: String,
        token: String,
    },
    /// Hesap el sıkışmaları: tek cevap (`auth` ya da `error`), sonra bağlantı kapanır.
    Register {
        code: String,
        username: String,
        password: String,
        device: String,
        /// p0.18: sunucunun fotoğraf klasöründen bir kimlik (boşsa fotoğrafsız).
        #[serde(skip_serializing_if = "String::is_empty", default)]
        avatar: String,
    },
    Login {
        username: String,
        password: String,
        device: String,
    },
    Logout {
        auth: String,
    },
    /// Tek seferlik: karşılıklı arkadaşların istatistikleri ve rozetler (`all`/`month`/`week`).
    Stats {
        auth: String,
        period: String,
    },
    /// Tek seferlik (p0.23): İzleyeceklerim / İzlediklerim (`op` boş = oku).
    Lists {
        auth: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        op: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        user: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        list: String,
        #[serde(rename = "ref", skip_serializing_if = "String::is_empty")]
        reference: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        rating: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        entry: Option<serde_json::Value>,
    },
    /// Tek seferlik (p0.22): ana ekran "Etkinlik" sekmesi, son 7 gün.
    Feed {
        auth: String,
    },
    /// Tek seferlik (p0.16): arkadaş listesi; `op` `list` | `add` | `remove`, `user` hesap adı.
    Friends {
        auth: String,
        op: String,
        #[serde(skip_serializing_if = "String::is_empty")]
        user: String,
    },
    /// Tek seferlik (p0.18): profil; `op` `get` | `avatar` | `invite`.
    Profile {
        auth: String,
        #[serde(skip_serializing_if = "String::is_empty", default)]
        op: String,
        #[serde(skip_serializing_if = "String::is_empty", default)]
        avatar: String,
        /// p0.19 `op: upload` (yalnızca admin): base64 görsel.
        #[serde(skip_serializing_if = "String::is_empty", default)]
        image: String,
    },
    /// Tek seferlik (p0.19, yalnızca admin): odalar, üyeler, oda kapatma, ban, davet hakkı.
    Admin {
        auth: String,
        #[serde(skip_serializing_if = "String::is_empty", default)]
        op: String,
        #[serde(skip_serializing_if = "String::is_empty", default)]
        room: String,
        #[serde(skip_serializing_if = "String::is_empty", default)]
        user: String,
        #[serde(skip_serializing_if = "std::ops::Not::not", default)]
        on: bool,
    },
    /// Lobi (p0.18): uygulama açıkken açık kalır, sunucu `presence` iter.
    Hello {
        auth: String,
    },
    /// Tek seferlik: hata bildirimi (p0.14). Hesaplı sunucuda `auth`, hesapsızda `username`.
    BugReport {
        #[serde(skip_serializing_if = "Option::is_none")]
        auth: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        version: String,
        text: String,
    },
    Cmd(Cmd),
    Report(Report),
    Ping {
        t0: i64,
        /// Son ölçülen RTT (ms). Sunucu countdown payı (`startAt`) için kullanır.
        #[serde(skip_serializing_if = "Option::is_none")]
        rtt: Option<i64>,
    },
    /// Odadan bilerek ayrılma (bağlantı kopmasından farklı: hayalet kalınmaz).
    Leave,
    /// "Herkes İndirsin" görevindeki yerel ilerleme (en çok 1 Hz).
    DlProgress(DlProgress),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DlProgress {
    pub task_id: String,
    pub state: DlState,
    pub pct: f64,
    /// bayt/s
    #[serde(default, skip_serializing_if = "is_zero")]
    pub speed: f64,
    /// s
    #[serde(default, skip_serializing_if = "is_zero")]
    pub eta: f64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DlState {
    Queued,
    Downloading,
    Ready,
    Failed,
}

/// Bir üyenin indirme durumu (`dl_status`, `snapshot.downloads`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DlStatus {
    pub user: String,
    pub name: String,
    pub state: DlState,
    pub pct: f64,
    #[serde(default)]
    pub speed: f64,
    #[serde(default)]
    pub eta: f64,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cmd {
    pub cid: String,
    pub op: CmdOp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<Policy>,
    /// Yalnızca `op: media`. Kutulu: `ClientMsg` küçük kalsın.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Box<Media>>,
    /// `op: continue_without` (beklenmeyecek üye) ve `op: set_cohost` (hedef üye).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Yalnızca `op: set_cohost`: yardımcı host yap (`true`) / al (`false`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<bool>,
}

/// Odanın oynattığı medya (`cmd op=media`, `state.media`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Media {
    pub kind: MediaKind,
    /// `url`/`download`: http(s) adresi, her istemci kendisi çözer/indirir. `file`: yalnızca
    /// dosya adı. `debrid`: torrent infoHash'i; herkes kendi Real-Debrid hesabıyla çözer.
    pub src: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Saniye; bilinmiyorsa 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub duration: f64,
    /// `download`: sunucunun verdiği görev kimliği (`dl_progress.taskId`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub task: String,
    /// `url`: yakalanan akışın geldiği sayfa; yüklerken HTTP Referer olarak gönderilir.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub referer: String,
    /// `url` + `referer`: yakalayıcının sayfada gördüğü altyazı dosyaları (p0.9).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subs: Vec<SubFile>,
    /// `debrid` (p0.15): torrentteki dosyanın adı (ipucu, yol değil).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    /// `debrid` (p0.15): Stremio `fileIdx` (torrentteki 0 tabanlı sıra).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idx: Option<u32>,
}

impl Media {
    /// Yalnızca tür, kaynak ve başlıkla; diğer alanlar boş.
    pub fn new(kind: MediaKind, src: String, title: String) -> Self {
        Self {
            kind,
            src,
            title,
            duration: 0.0,
            task: String::new(),
            referer: String::new(),
            subs: Vec::new(),
            file: String::new(),
            idx: None,
        }
    }
}

/// Yakalanan akışın altyazı dosyası (`media.subs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubFile {
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lang: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Url,
    File,
    /// "Herkes İndirsin": herkes indirir, hazır olunca oda birlikte başlar.
    Download,
    /// Real-Debrid (p0.15): `src` infoHash; herkes kendi hesabıyla çözer.
    Debrid,
    /// Tarayıcıda senkron (p0.20): `src` web sayfası; herkes Keşfet'te açar, sitenin
    /// kendi `<video>`'su oda saatine uyar.
    Page,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmdOp {
    Play,
    Pause,
    Seek,
    SetPolicy,
    Media,
    /// Host: kronik stall eden üyeyi beklemeden devam et (üye DETACHED olur).
    ContinueWithout,
    /// DETACHED üye: "Odayla Senkronla".
    Resync,
    /// Host: üyeyi yardımcı host yap / al (`user`, `on`).
    SetCohost,
    /// p0.21: odadaki medyayı kapat (oda boşalır, oda kapanmaz).
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    Free,
    HostOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub pos: f64,
    pub state: ReportState,
    pub buffer_sec: f64,
    pub buffer_pct: u8,
    /// > 30 s geride kalıp ileri seek yapıldıysa atlanan süre (s).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_sec: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportState {
    Playing,
    Paused,
    Stalled,
    Refilling,
    Ready,
    AheadHold,
    CatchingUp,
    Detached,
}

// ---------------------------------------------------------------- Sunucu → İstemci

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    RoomCreated {
        room: String,
        name: String,
        invite: String,
        you: String,
    },
    Joined {
        you: String,
        invite: String,
        snapshot: Box<Snapshot>,
        #[serde(rename = "serverTs")]
        server_ts: i64,
        /// Yeniden bağlanma belirteci (`resume.token`).
        #[serde(default)]
        resume: Option<String>,
        /// `resume` ile dönüldüyse true.
        #[serde(default)]
        resumed: bool,
    },
    State(RoomState),
    Activity {
        kind: String,
        #[serde(default)]
        user: Option<String>,
        text: String,
        #[serde(rename = "serverTs", default)]
        server_ts: Option<i64>,
    },
    MemberJoined {
        member: Member,
    },
    MemberLeft {
        id: String,
        /// Ayrılan host'tu: yeni host'un id'si.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host: Option<String>,
    },
    /// Üyenin bağlantısı koptu; 60 sn içinde dönebilir.
    MemberAway {
        id: String,
    },
    MemberBack {
        id: String,
    },
    /// Üye odayı beklemeden izleme durumuna girdi/çıktı.
    MemberDetached {
        id: String,
        detached: bool,
    },
    /// Host bir üyeyi yardımcı host yaptı ya da bunu geri aldı.
    MemberRole {
        id: String,
        cohost: bool,
    },
    /// Host oda politikasını değiştirdi.
    Policy {
        policy: Policy,
    },
    /// Yalnızca host'a: üye kronik stall ediyor, onsuz devam edilebilir.
    DetachOffer {
        user: String,
        name: String,
        stalls: u32,
    },
    /// Bu istemci DETACHED yapıldı (host "onsuz devam" dedi).
    Detached {
        #[serde(default)]
        by: Option<String>,
    },
    MemberStatus(MemberStatus),
    DlStatus {
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(flatten)]
        status: DlStatus,
    },
    BufferStatus {
        user: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(rename = "bufferPct")]
        buffer_pct: u8,
        #[serde(rename = "targetSec")]
        target_sec: f64,
    },
    Pong {
        t0: i64,
        #[serde(rename = "serverTs")]
        server_ts: i64,
    },
    Denied {
        ack: String,
        reason: String,
    },
    Error {
        code: String,
        #[serde(default)]
        message: String,
    },
    /// `register`/`login` başarılı (`token` hesap oturumu, `name` hesap adı); `logout`'ta boş.
    Auth {
        token: String,
        #[serde(default)]
        name: String,
        /// p0.18: `Ad#etiket` etiketi ve profil fotoğrafı.
        #[serde(default)]
        tag: String,
        #[serde(default)]
        avatar: String,
    },
    /// İleride eklenecek mesaj tipleri. Motor bunları yok sayar.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub room: String,
    pub name: String,
    pub policy: Policy,
    /// p0.13 oda modu: `browse` | `watch` | `listen` (eski sunucuda boş).
    #[serde(default)]
    pub view: String,
    pub host: String,
    pub members: Vec<Member>,
    pub state: RoomState,
    #[serde(default)]
    pub downloads: Vec<DlStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub host: bool,
    #[serde(default)]
    pub cohost: bool,
    #[serde(default)]
    pub away: bool,
    #[serde(default)]
    pub detached: bool,
    /// p0.19: sunucu admini (ADMIN etiketi).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub admin: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberStatus {
    pub id: String,
    pub pos: f64,
    #[serde(default)]
    pub drift_ms: i64,
    pub state: ReportState,
    #[serde(default)]
    pub buffer_sec: f64,
    #[serde(default)]
    pub buffer_pct: u8,
    /// Üyenin ping'le bildirdiği son RTT (ms); bilinmiyorsa yok (p0.14).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomMode {
    Paused,
    Countdown,
    Playing,
    Hold,
}

/// Oda anchor'ı (`state` mesajı ve snapshot içindeki `state`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomState {
    pub seq: u64,
    pub mode: RoomMode,
    pub anchor_pos: f64,
    pub anchor_ts: i64,
    #[serde(default)]
    pub start_at: i64,
    #[serde(default = "one")]
    pub rate: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<By>,
    /// `play` | `pause` | `seek` | `start` | `stall` | `release` | `snapshot` | `catch_up` …
    #[serde(default)]
    pub cause: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ack: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_ts: Option<i64>,
    // HOLD alanları
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold_pos: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waiting_for: Vec<String>,
    /// `stall` (Faz 0) | `catch_up` (ileride).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Kutulu: `ServerMsg::State` varyantını küçük tutar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Box<Media>>,
}

fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct By {
    pub id: String,
    pub name: String,
}

impl RoomState {
    /// PROTOCOL §6: `position(t) = anchorPos + (playing ? (t − anchorTs)/1000 × rate : 0)`.
    pub fn position_at(&self, server_ms: f64) -> f64 {
        match self.mode {
            RoomMode::Playing => {
                self.anchor_pos + (server_ms - self.anchor_ts as f64) / 1000.0 * self.rate
            }
            _ => self.anchor_pos,
        }
    }
}

/// `bufferPct = min(100, bufferSec / targetSec × 100)`.
pub fn buffer_pct(buffer_sec: f64, target_sec: f64) -> u8 {
    if target_sec <= 0.0 {
        return 100;
    }
    (buffer_sec / target_sec * 100.0).clamp(0.0, 100.0).floor() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_status_rtt_optional() {
        let with = r#"{"t":"member_status","id":"u2","pos":1.5,"driftMs":-20,"state":"playing","bufferSec":12,"bufferPct":100,"rttMs":84}"#;
        let ServerMsg::MemberStatus(m) = serde_json::from_str(with).unwrap() else {
            panic!("member_status bekleniyordu");
        };
        assert_eq!(m.rtt_ms, Some(84));
        // UI'a giden yeniden serileştirmede alan korunur.
        let back = serde_json::to_string(&ServerMsg::MemberStatus(m)).unwrap();
        assert!(back.contains("\"rttMs\":84"), "{back}");

        let without = r#"{"t":"member_status","id":"u2","pos":1.5,"state":"playing"}"#;
        let ServerMsg::MemberStatus(m) = serde_json::from_str(without).unwrap() else {
            panic!("member_status bekleniyordu");
        };
        assert_eq!(m.rtt_ms, None);
    }
}
