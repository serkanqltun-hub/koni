//! Tarayıcıda senkron (p0.20, oda medyası `page`): yt-dlp'nin tanımadığı sitede video
//! KONİ oynatıcısına alınmaz, Keşfet'in tarayıcısındaki sitenin kendi `<video>`'su
//! oda saatine uyar. Sayfaya enjekte edilen `pagesync.js` videonun durumunu
//! `page_video` ile bildirir; senkron motoru `sync.rs`'teki oynatıcı uyarlayıcısı
//! üzerinden bu durumu okur ve komutları (`play`/`pause`/`seek`/`rate`) sayfaya yollar.
//!
//! Sayfada kullanıcının kendi bastığı oynat/durdur/sar (sitenin oynatıcısında) odaya
//! `UiCmd` olarak gider; KONİ'nin kendi verdiği komutların yankısını betik ayıklar.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::sniffer;

/// Son bildirimden bu kadar geçtiyse sayfada video yok sayılır (betik 0,5 sn'de bir yollar).
const STALE: Duration = Duration::from_millis(2500);

#[derive(Clone, Copy)]
struct Video {
    pos: f64,
    at: Instant,
    paused: bool,
    rate: f64,
    dur: Option<f64>,
    buffering: bool,
    ended: bool,
}

#[derive(Default)]
struct Page {
    /// Oda medyası `page` iken sayfanın adresi; `None`: oda mpv'yi kullanır.
    room: Option<String>,
    video: Option<Video>,
    /// Sayfanın başlığı (`document.title`), odaya giden adı.
    title: String,
}

static PAGE: Mutex<Page> = Mutex::new(Page {
    room: None,
    video: None,
    title: String::new(),
});

/// Oda medyası değişti: `page` ise adresi, değilse `None`.
pub fn set_room(url: Option<String>) {
    PAGE.lock().unwrap().room = url;
}

/// Senkron motoru sayfanın videosunu mu sürüyor.
pub fn active() -> bool {
    PAGE.lock().unwrap().room.is_some()
}

fn video() -> Option<Video> {
    PAGE.lock()
        .unwrap()
        .video
        .filter(|v| v.at.elapsed() < STALE)
}

/// Şu anki konum: son bildirimden bu yana oynuyorsa ileri tahmin edilir.
pub fn position() -> f64 {
    video().map_or(0.0, |v| {
        if v.paused || v.buffering || v.ended {
            v.pos
        } else {
            v.pos + v.at.elapsed().as_secs_f64() * v.rate
        }
    })
}

/// Video yokken duraklamış ve "tamponluyor" sayılır: oda bu üyeyi bekler.
pub fn paused() -> bool {
    video().is_none_or(|v| v.paused)
}
pub fn buffering() -> bool {
    video().is_none_or(|v| v.buffering)
}
pub fn duration() -> Option<f64> {
    video().and_then(|v| v.dur)
}
pub fn ended() -> bool {
    video().is_some_and(|v| v.ended)
}

/// Motorun komutu sayfaya gider; yerel durum hemen güncellenir ki bir sonraki tick
/// aynı komutu tekrar vermesin (sayfanın bildirimi ~0,5 sn sonra gelir).
pub fn command(app: &AppHandle, op: &str, value: f64) {
    {
        let mut p = PAGE.lock().unwrap();
        if let Some(v) = p.video.as_mut() {
            let now = position_of(v);
            match op {
                "play" => v.paused = false,
                "pause" => v.paused = true,
                "seek" => v.pos = value,
                "rate" => v.rate = value,
                _ => {}
            }
            if op != "seek" {
                v.pos = now;
            }
            v.at = Instant::now();
        }
    }
    if let Some(w) = app.get_webview_window(sniffer::LABEL) {
        let js = format!("window.__koniPage&&window.__koniPage({{op:{op:?},v:{value}}})");
        let _ = w.eval(js);
    }
}

fn position_of(v: &Video) -> f64 {
    if v.paused || v.buffering || v.ended {
        v.pos
    } else {
        v.pos + v.at.elapsed().as_secs_f64() * v.rate
    }
}

/// `pagesync.js`'in bildirimi.
#[derive(Deserialize)]
pub struct Report {
    pos: f64,
    paused: bool,
    #[serde(default = "one")]
    rate: f64,
    dur: Option<f64>,
    #[serde(default)]
    buffering: bool,
    #[serde(default)]
    ended: bool,
    /// Kullanıcı sitenin oynatıcısında bastı: `play` | `pause` | `seek`.
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

fn one() -> f64 {
    1.0
}

/// Kaydeder; kullanıcının kendi hareketini döner (oda `page` iken odaya gider).
pub fn report(r: Report) -> Option<(String, f64)> {
    let ok = |x: f64| x.is_finite() && x >= 0.0;
    if !ok(r.pos) {
        return None;
    }
    let mut p = PAGE.lock().unwrap();
    if let Some(t) = r.title.filter(|t| !t.trim().is_empty()) {
        p.title = t.trim().chars().take(200).collect();
    }
    p.video = Some(Video {
        pos: r.pos,
        at: Instant::now(),
        paused: r.paused,
        rate: if ok(r.rate) && r.rate > 0.0 {
            r.rate
        } else {
            1.0
        },
        dur: r.dur.filter(|d| ok(*d) && *d > 0.0),
        buffering: r.buffering,
        ended: r.ended,
    });
    let user = r
        .user
        .filter(|u| matches!(u.as_str(), "play" | "pause" | "seek"));
    match (&p.room, user) {
        (Some(_), Some(u)) => Some((u, r.pos)),
        _ => None,
    }
}

/// Keşfet'in adres çubuğu için sayfanın durumu (`browse_state`).
#[derive(Serialize)]
pub struct VideoState {
    pub pos: f64,
    pub dur: Option<f64>,
    pub paused: bool,
}

pub fn state() -> Option<VideoState> {
    video().map(|v| VideoState {
        pos: position_of(&v),
        dur: v.dur,
        paused: v.paused,
    })
}

pub fn title() -> String {
    PAGE.lock().unwrap().title.clone()
}

/// Sayfa değişti: eski sayfanın videosu ve başlığı unutulur.
pub fn forget_video() {
    let mut p = PAGE.lock().unwrap();
    p.video = None;
    p.title.clear();
}
