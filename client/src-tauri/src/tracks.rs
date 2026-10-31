//! Kalite · ses (dublaj) · altyazı seçimi.
//!
//! İzler iki yerden gelir: mpv'nin kendi bulduğu izler (m3u8 master'daki çözünürlükler,
//! EXT-X-MEDIA ses/altyazıları, dosyanın içindekiler) ve yt-dlp'nin verdiği alternatif
//! adresler (YouTube kaliteleri, dublaj dilleri). Alternatif adres ancak seçilince
//! `video-add`/`audio-add` ile açılır, oynatma durmaz. Seçim kişiseldir: odaya gitmez.

use crate::i18n::trs;
use std::sync::{Arc, Mutex};

use koni_player::Player;
use koni_resolver::Alt;
use koni_sync::protocol::SubFile;
use serde::Serialize;

use crate::media::Stream;

struct Current {
    video: String,
    videos: Vec<Alt>,
    audios: Vec<Alt>,
    cover: Option<String>,
    /// Dosya açılınca eklenecek (`on_file_loaded`), sonra boşalır.
    pending_subs: Vec<SubFile>,
}

static CURRENT: Mutex<Option<Current>> = Mutex::new(None);

/// `media::load` çağırır: yüklenen akışın alternatifleri.
pub fn set_current(s: &Stream) {
    *CURRENT.lock().unwrap() = Some(Current {
        video: s.video.clone(),
        videos: s.videos.clone(),
        audios: s.audios.clone(),
        cover: s.cover.clone(),
        pending_subs: s.subs.clone(),
    });
}

/// Yerel dosya açıldı: alternatif yok.
pub fn clear() {
    *CURRENT.lock().unwrap() = None;
}

/// Şu an çalan akışın yt-dlp kapağı (yerel dosyada yok).
pub fn cover(mpv: &Player) -> Option<String> {
    let cur = CURRENT.lock().unwrap();
    let c = cur.as_ref()?;
    (mpv.get_property_string("path").as_deref() == Some(c.video.as_str()))
        .then(|| c.cover.clone())
        .flatten()
}

/// mpv `file-loaded`: bekleyen altyazıları ayrı thread'de ekle (her biri indirilir;
/// olay döngüsü beklemesin). Hiç altyazı seçili değilse ilk Türkçe altyazı seçilir.
pub fn on_file_loaded(mpv: &Arc<Player>) {
    // Akış açıldı: zorlanan biçim (`media::load`, HLS) sonradan eklenen altyazıları
    // da HLS diye açtırır; temizlenir.
    if mpv
        .get_property_string("demuxer-lavf-format")
        .is_some_and(|f| !f.is_empty())
    {
        let _ = mpv.set_property_string("file-local-options/demuxer-lavf-format", "");
    }
    let subs = {
        let mut cur = CURRENT.lock().unwrap();
        match cur.as_mut() {
            Some(c) if mpv.get_property_string("path").as_deref() == Some(c.video.as_str()) => {
                std::mem::take(&mut c.pending_subs)
            }
            _ => return,
        }
    };
    if subs.is_empty() {
        return;
    }
    let mpv = mpv.clone();
    std::thread::spawn(move || add_subs(&mpv, subs));
}

fn add_subs(mpv: &Player, subs: Vec<SubFile>) {
    let has_tr = read_tracks(mpv).iter().any(|t| {
        t.kind == "sub"
            && t.selected
            && (t.lang.eq_ignore_ascii_case("tr")
                || t.lang.eq_ignore_ascii_case("tur")
                || t.title.to_lowercase().contains("türk")
                || t.title.to_lowercase().contains("turk"))
    });
    let mut pick = !has_tr;
    for s in subs {
        let tr = s.lang.to_lowercase().starts_with("tr");
        let flag = if pick && tr { "select" } else { "auto" };
        let title = if s.title.is_empty() {
            &s.lang
        } else {
            &s.title
        };
        if mpv
            .command(&["sub-add", &s.url, flag, title, &s.lang])
            .is_ok()
            && flag == "select"
        {
            pick = false;
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Item {
    /// mpv iz kimliği, `alt:N` (henüz açılmamış alternatif) ya da `no` (kapalı).
    pub key: String,
    pub label: String,
    pub selected: bool,
}

#[derive(Serialize, Default, Debug)]
pub struct Tracks {
    pub video: Vec<Item>,
    pub audio: Vec<Item>,
    pub sub: Vec<Item>,
}

/// `track-list` girdisi (gerekli alanlar).
#[derive(Clone, Debug, Default)]
struct T {
    id: String,
    kind: String,
    title: String,
    lang: String,
    selected: bool,
    external: Option<String>,
    height: u32,
    bitrate: u64,
}

fn read_tracks(mpv: &Player) -> Vec<T> {
    let n: usize = mpv
        .get_property_string("track-list/count")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let get = |i: usize, k: &str| mpv.get_property_string(&format!("track-list/{i}/{k}"));
    (0..n)
        .filter(|&i| get(i, "albumart").as_deref() != Some("yes"))
        .map(|i| T {
            id: get(i, "id").unwrap_or_default(),
            kind: get(i, "type").unwrap_or_default(),
            title: get(i, "title").unwrap_or_default(),
            lang: get(i, "lang").unwrap_or_default(),
            selected: get(i, "selected").as_deref() == Some("yes"),
            external: get(i, "external-filename").filter(|f| !f.is_empty()),
            height: get(i, "demux-h").and_then(|s| s.parse().ok()).unwrap_or(0),
            bitrate: get(i, "hls-bitrate")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        })
        .collect()
}

/// Bir alternatif adresin mpv izi: ana adres dosyanın kendi izi, diğerleri eklenmiş iz.
fn alt_track<'a>(ts: &'a [T], kind: &str, url: &str, main: Option<&str>) -> Option<&'a T> {
    ts.iter()
        .filter(|t| t.kind == kind)
        .find(|t| match &t.external {
            Some(f) => f == url,
            None => main == Some(url),
        })
}

fn alt_items(ts: &[T], kind: &str, alts: &[Alt], main: Option<&str>) -> Vec<Item> {
    alts.iter()
        .enumerate()
        .map(|(i, a)| match alt_track(ts, kind, &a.url, main) {
            Some(t) => Item {
                key: t.id.clone(),
                label: a.label.clone(),
                selected: t.selected,
            },
            None => Item {
                key: format!("alt:{i}"),
                label: a.label.clone(),
                selected: false,
            },
        })
        .collect()
}

fn mbps(b: u64) -> String {
    format!("{:.1} Mbps", b as f64 / 1e6)
}

fn name(t: &T, fallback: &str) -> String {
    match (t.title.is_empty(), t.lang.is_empty()) {
        (false, false) => format!("{} ({})", t.title, t.lang),
        (false, true) => t.title.clone(),
        (true, false) => t.lang.clone(),
        (true, true) => format!("{fallback} {}", t.id),
    }
}

/// m3u8 kaliteleri: her çözünürlük bir video izi.
fn video_items(ts: &[T]) -> Vec<Item> {
    let mut v: Vec<&T> = ts.iter().filter(|t| t.kind == "video").collect();
    v.sort_by_key(|t| std::cmp::Reverse((t.height, t.bitrate)));
    v.iter()
        .map(|t| {
            let label = match (t.height, t.bitrate) {
                (0, 0) => name(t, "Video"),
                (0, b) => mbps(b),
                (h, b) if v.iter().filter(|o| o.height == h).count() > 1 && b > 0 => {
                    format!("{h}p · {}", mbps(b))
                }
                (h, _) => format!("{h}p"),
            };
            Item {
                key: t.id.clone(),
                label,
                selected: t.selected,
            }
        })
        .collect()
}

/// m3u8'de her kalitenin kendi (aynı) sesi olabilir: bunlar tek satır olur. Ayrı
/// ses izleri (dublaj) ayrı satırdır.
fn audio_items(ts: &[T]) -> Vec<Item> {
    let rates: Vec<u64> = ts
        .iter()
        .filter(|t| t.kind == "video" && t.bitrate > 0)
        .map(|t| t.bitrate)
        .collect();
    let muxed = |t: &T| t.bitrate > 0 && rates.contains(&t.bitrate);
    let mut out: Vec<Item> = Vec::new();
    let mut groups: Vec<(String, usize)> = Vec::new(); // (dil+başlık, out sırası)
    for t in ts.iter().filter(|t| t.kind == "audio") {
        let group = format!("{}\u{1}{}", t.title, t.lang);
        if muxed(t) {
            if let Some(&(_, i)) = groups.iter().find(|(g, _)| *g == group) {
                if t.selected {
                    out[i].key = t.id.clone();
                    out[i].selected = true;
                }
                continue;
            }
            groups.push((group, out.len()));
        }
        out.push(Item {
            key: t.id.clone(),
            label: name(t, trs("Ses", "Audio")),
            selected: t.selected,
        });
    }
    out
}

fn sub_items(ts: &[T]) -> Vec<Item> {
    let mut v = vec![Item {
        key: "no".into(),
        label: trs("Kapalı", "Off").into(),
        selected: !ts.iter().any(|t| t.kind == "sub" && t.selected),
    }];
    v.extend(ts.iter().filter(|t| t.kind == "sub").map(|t| Item {
        key: t.id.clone(),
        label: name(t, trs("Altyazı", "Subtitles")),
        selected: t.selected,
    }));
    v
}

fn build(ts: &[T], cur: Option<&Current>) -> Tracks {
    let video = match cur {
        Some(c) if !c.videos.is_empty() => alt_items(ts, "video", &c.videos, Some(&c.video)),
        _ => video_items(ts),
    };
    let audio = match cur {
        // Ayrı ses dosyası (`audio-files-append`) da eklenmiş iz sayılır.
        Some(c) if !c.audios.is_empty() => alt_items(ts, "audio", &c.audios, None),
        _ => audio_items(ts),
    };
    Tracks {
        video,
        audio,
        sub: sub_items(ts),
    }
}

pub fn list(mpv: &Player) -> Tracks {
    let ts = read_tracks(mpv);
    build(&ts, CURRENT.lock().unwrap().as_ref())
}

/// `kind`: `video` | `audio` | `sub`; `key`: `list`'in verdiği anahtar.
pub fn select(mpv: &Player, kind: &str, key: &str) -> Result<(), String> {
    let prop = match kind {
        "video" => "vid",
        "audio" => "aid",
        "sub" => "sid",
        _ => {
            return Err(tr!(
                "geçersiz iz türü: {kind}",
                "invalid track type: {kind}"
            ))
        }
    };
    if let Some(i) = key.strip_prefix("alt:") {
        let alt = {
            let cur = CURRENT.lock().unwrap();
            let c = cur
                .as_ref()
                .ok_or(trs("alternatif yok", "no alternatives"))?;
            let alts = if kind == "video" {
                &c.videos
            } else {
                &c.audios
            };
            i.parse::<usize>()
                .ok()
                .and_then(|i| alts.get(i).cloned())
                .ok_or(trs("alternatif bulunamadı", "alternative not found"))?
        };
        let cmd = if kind == "video" {
            "video-add"
        } else {
            "audio-add"
        };
        return mpv
            .command(&[cmd, &alt.url, "select", &alt.label, &alt.lang])
            .map_err(|e| e.to_string());
    }
    if key != "no" && key.parse::<u32>().is_err() {
        return Err(tr!("geçersiz iz: {key}", "invalid track: {key}"));
    }
    if key == "no" && kind != "sub" {
        return Err(trs(
            "yalnızca altyazı kapatılabilir",
            "only subtitles can be turned off",
        )
        .into());
    }
    let before = (kind == "video").then(|| read_tracks(mpv));
    mpv.set_property_string(prop, key)
        .map_err(|e| e.to_string())?;
    if let Some(ts) = before {
        follow_muxed_audio(mpv, &ts, key);
    }
    Ok(())
}

/// m3u8 kalitesi değişti ve ses o kalitenin içindeyse: sesi de yeni kaliteninkine geçir
/// (yoksa iki kalite birden indirilir).
fn follow_muxed_audio(mpv: &Player, ts: &[T], vid: &str) {
    let rate = |id: &str| {
        ts.iter()
            .find(|t| t.kind == "video" && t.id == id)
            .map(|t| t.bitrate)
    };
    let Some(old) = ts.iter().find(|t| t.kind == "video" && t.selected) else {
        return;
    };
    let (Some(new_rate), Some(a)) = (
        rate(vid),
        ts.iter().find(|t| t.kind == "audio" && t.selected),
    ) else {
        return;
    };
    if old.bitrate == 0 || a.bitrate != old.bitrate || new_rate == 0 {
        return;
    }
    if let Some(n) = ts.iter().find(|t| {
        t.kind == "audio" && t.bitrate == new_rate && t.lang == a.lang && t.title == a.title
    }) {
        let _ = mpv.set_property_string("aid", &n.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, kind: &str, h: u32, br: u64, sel: bool) -> T {
        T {
            id: id.into(),
            kind: kind.into(),
            height: h,
            bitrate: br,
            selected: sel,
            ..T::default()
        }
    }

    #[test]
    fn hls_variants_become_qualities_and_muxed_audio_one_row() {
        let ts = vec![
            t("1", "video", 1080, 5_000_000, true),
            t("1", "audio", 0, 5_000_000, true),
            t("2", "video", 720, 2_500_000, false),
            t("2", "audio", 0, 2_500_000, false),
            T {
                lang: "tr".into(),
                ..t("3", "audio", 0, 0, false)
            },
            T {
                lang: "en".into(),
                ..t("1", "sub", 0, 0, false)
            },
        ];
        let r = build(&ts, None);
        let v: Vec<_> = r
            .video
            .iter()
            .map(|i| (i.key.as_str(), i.label.as_str(), i.selected))
            .collect();
        assert_eq!(v, [("1", "1080p", true), ("2", "720p", false)]);
        let a: Vec<_> = r
            .audio
            .iter()
            .map(|i| (i.key.as_str(), i.label.as_str()))
            .collect();
        assert_eq!(a, [("1", "Ses 1"), ("3", "tr")]);
        let s: Vec<_> = r.sub.iter().map(|i| (i.key.as_str(), i.selected)).collect();
        assert_eq!(s, [("no", true), ("1", false)]);
    }

    #[test]
    fn ytdlp_alternatives_map_to_tracks() {
        let alt = |l: &str, u: &str| Alt {
            label: l.into(),
            url: u.into(),
            lang: String::new(),
        };
        let cur = Current {
            video: "https://v/1080".into(),
            videos: vec![alt("1080p", "https://v/1080"), alt("720p", "https://v/720")],
            audios: vec![
                alt("English", "https://a/en"),
                alt("Türkçe", "https://a/tr"),
            ],
            cover: None,
            pending_subs: Vec::new(),
        };
        let ts = vec![
            t("1", "video", 1080, 0, true),
            T {
                external: Some("https://a/en".into()),
                ..t("1", "audio", 0, 0, true)
            },
        ];
        let r = build(&ts, Some(&cur));
        let v: Vec<_> = r
            .video
            .iter()
            .map(|i| (i.key.as_str(), i.selected))
            .collect();
        assert_eq!(v, [("1", true), ("alt:1", false)]);
        let a: Vec<_> = r
            .audio
            .iter()
            .map(|i| (i.key.as_str(), i.selected))
            .collect();
        assert_eq!(a, [("1", true), ("alt:1", false)]);
    }
}
