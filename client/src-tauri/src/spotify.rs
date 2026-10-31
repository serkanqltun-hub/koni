//! Spotify linkleri (parça, albüm, çalma listesi). Spotify'ın sesi DRM'li, doğrudan
//! çalınamaz: herkese açık embed sayfasından şarkı adları okunur, her biri YouTube'da
//! aranıp ilk sonuç çalınır. Anahtar ya da hesap gerekmez.

use crate::i18n::trs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use koni_resolver::Hit;

/// Bir listeden en fazla bu kadar şarkı eşlenir.
const MAX_TRACKS: usize = 100;
/// Aynı anda çalışan YouTube aramaları.
const WORKERS: usize = 8;

/// `open.spotify.com/(intl-xx/)?{track|album|playlist}/ID` ya da `spotify:track:ID`.
pub fn parse(url: &str) -> Option<(&'static str, String)> {
    let s = url.trim();
    let rest = if let Some(r) = s.strip_prefix("spotify:") {
        r.replacen(':', "/", 1)
    } else {
        let i = s.find("open.spotify.com/")?;
        s[i + "open.spotify.com/".len()..].to_string()
    };
    let mut parts = rest.split(['/', '?', '#']).filter(|p| !p.is_empty());
    let mut kind = parts.next()?;
    if kind.starts_with("intl-") {
        kind = parts.next()?;
    }
    let kind = match kind {
        "track" => "track",
        "album" => "album",
        "playlist" => "playlist",
        _ => return None,
    };
    let id = parts.next()?;
    id.chars()
        .all(|c| c.is_ascii_alphanumeric())
        .then(|| (kind, id.to_string()))
}

#[derive(Debug, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub dur: Option<f64>,
}

fn http() -> &'static reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            // embed sayfası tarayıcı olmayan istemciye boş iskelet dönebiliyor
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")
            .build()
            .expect("HTTP istemcisi kurulamadı")
    });
    &CLIENT
}

/// Linkteki şarkılar (embed sayfasının `__NEXT_DATA__` verisinden).
pub async fn tracks(kind: &str, id: &str) -> Result<Vec<Track>, String> {
    let url = format!("https://open.spotify.com/embed/{kind}/{id}");
    let res = http()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Spotify: {e}"))?;
    if !res.status().is_success() {
        return Err(format!(
            "Spotify: HTTP {} (liste gizli olabilir)",
            res.status().as_u16()
        ));
    }
    let html = res.text().await.map_err(|e| format!("Spotify: {e}"))?;
    let list = from_html(&html)?;
    if list.is_empty() {
        return Err(trs(
            "Spotify: bu linkte şarkı bulunamadı",
            "Spotify: no songs found at this link",
        )
        .into());
    }
    Ok(list)
}

fn from_html(html: &str) -> Result<Vec<Track>, String> {
    const START: &str = r#"<script id="__NEXT_DATA__" type="application/json">"#;
    let i = html.find(START).ok_or(trs(
        "Spotify sayfası okunamadı",
        "Couldn't read the Spotify page",
    ))? + START.len();
    let j = html[i..].find("</script>").ok_or(trs(
        "Spotify sayfası okunamadı",
        "Couldn't read the Spotify page",
    ))? + i;
    let v: serde_json::Value = serde_json::from_str(&html[i..j]).map_err(|_| {
        trs(
            "Spotify sayfası okunamadı",
            "Couldn't read the Spotify page",
        )
        .to_string()
    })?;
    Ok(from_entity(
        &v["props"]["pageProps"]["state"]["data"]["entity"],
    ))
}

fn from_entity(e: &serde_json::Value) -> Vec<Track> {
    let s = |v: &serde_json::Value| v.as_str().unwrap_or("").trim().to_string();
    let ms = |v: &serde_json::Value| v.as_f64().map(|d| d / 1000.0);
    if let Some(list) = e["trackList"].as_array() {
        return list
            .iter()
            .map(|t| Track {
                title: s(&t["title"]),
                artist: s(&t["subtitle"]).replace('\u{a0}', " "),
                dur: ms(&t["duration"]),
            })
            .filter(|t| !t.title.is_empty())
            .take(MAX_TRACKS)
            .collect();
    }
    let title = Some(s(&e["name"]))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| s(&e["title"]));
    let artist = e["artists"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|x| s(&x["name"]))
                .filter(|n| !n.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| s(&e["subtitle"]));
    if title.is_empty() {
        return Vec::new();
    }
    vec![Track {
        title,
        artist,
        dur: ms(&e["duration"]),
    }]
}

/// Her şarkı YouTube'da aranır (sıra korunur); bulunamayan atlanır.
pub fn match_youtube(list: Vec<Track>, search: impl Fn(&str) -> Option<Hit> + Sync) -> Vec<Hit> {
    let out: Vec<Mutex<Option<Hit>>> = list.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|sc| {
        for _ in 0..WORKERS.min(list.len()) {
            sc.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(t) = list.get(i) else { break };
                let name = if t.artist.is_empty() {
                    t.title.clone()
                } else {
                    format!("{} - {}", t.artist, t.title)
                };
                if let Some(mut h) = search(&format!("{name} audio")) {
                    h.title = name;
                    h.duration_secs = t.dur.or(h.duration_secs);
                    *out[i].lock().unwrap() = Some(h);
                }
            });
        }
    });
    out.into_iter()
        .filter_map(|m| m.into_inner().unwrap())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_links() {
        assert_eq!(
            parse("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=ab"),
            Some(("playlist", "37i9dQZF1DXcBWIGoYBM5M".into()))
        );
        assert_eq!(
            parse("https://open.spotify.com/intl-tr/track/0VjIjW4GlUZAMYd2vXMi3b"),
            Some(("track", "0VjIjW4GlUZAMYd2vXMi3b".into()))
        );
        assert_eq!(
            parse("spotify:album:4yP0hdKOZPNshxUOjY0cZj"),
            Some(("album", "4yP0hdKOZPNshxUOjY0cZj".into()))
        );
        assert_eq!(
            parse("https://open.spotify.com/artist/1Xyo4u8uXC1ZmMpatF05PJ"),
            None
        );
        assert_eq!(parse("https://www.youtube.com/watch?v=x"), None);
    }

    #[test]
    fn reads_playlist_and_track_entities() {
        let html = r#"<html><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"state":{"data":{"entity":{"name":"Liste","trackList":[{"title":"Blinding Lights","subtitle":"The Weeknd","duration":200040},{"title":"","subtitle":"x"}]}}}}}}</script></html>"#;
        assert_eq!(
            from_html(html).unwrap(),
            vec![Track {
                title: "Blinding Lights".into(),
                artist: "The Weeknd".into(),
                dur: Some(200.04)
            }]
        );
        let one = serde_json::json!({"name": "Get Lucky", "artists": [{"name": "Daft Punk"}, {"name": "Pharrell Williams"}], "duration": 248000});
        assert_eq!(
            from_entity(&one),
            vec![Track {
                title: "Get Lucky".into(),
                artist: "Daft Punk, Pharrell Williams".into(),
                dur: Some(248.0)
            }]
        );
    }
}
