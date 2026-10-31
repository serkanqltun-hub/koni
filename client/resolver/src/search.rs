//! YouTube search for the Keşfet screen: `yt-dlp --flat-playlist -J "ytsearchN:query"`,
//! and the songs behind a link for KONİ playlists. No API key; results are listed
//! without resolving each video (fast).

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::runner::CommandRunner;
use crate::{ResolveError, YtDlpConfig};

/// One search result.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub url: String,
    pub title: String,
    pub channel: String,
    pub duration_secs: Option<f64>,
    pub thumbnail: Option<String>,
    /// Live streams can't be synced (plan: live is out of scope); listed but flagged.
    pub live: bool,
}

#[derive(Deserialize)]
struct Flat {
    #[serde(default)]
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    id: Option<String>,
    url: Option<String>,
    title: Option<String>,
    channel: Option<String>,
    uploader: Option<String>,
    duration: Option<f64>,
    #[serde(default)]
    thumbnails: Vec<Thumb>,
    live_status: Option<String>,
}

#[derive(Deserialize)]
struct Page {
    entries: Option<Vec<Entry>>,
    webpage_url: Option<String>,
    #[serde(flatten)]
    info: Entry,
}

#[derive(Deserialize)]
struct Thumb {
    url: Option<String>,
    width: Option<u32>,
}

pub const MAX_HITS: usize = 20;
/// Longest playlist [`playlist`] reads; yt-dlp stops fetching pages there.
pub const MAX_LIST: usize = 500;

pub fn search(
    query: &str,
    count: usize,
    cfg: &YtDlpConfig,
    runner: &dyn CommandRunner,
) -> Result<Vec<Hit>, ResolveError> {
    let q: String = query
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let n = count.clamp(1, MAX_HITS);
    let args: Vec<String> = [
        "--flat-playlist",
        "-J",
        "--no-warnings",
        "--",
        &format!("ytsearch{n}:{q}"),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let out = runner.run(&cfg.binary, &args, Duration::from_secs(20))?;
    if !out.success {
        return Err(ResolveError::from_stderr(&out.stderr));
    }
    parse(&out.stdout)
}

/// Songs behind a link: every entry of a playlist (YouTube, SoundCloud…) or the single
/// video itself. Private and deleted playlist entries are skipped.
pub fn playlist(
    url: &str,
    cfg: &YtDlpConfig,
    runner: &dyn CommandRunner,
) -> Result<Vec<Hit>, ResolveError> {
    let end = MAX_LIST.to_string();
    let args: Vec<String> = [
        "--flat-playlist",
        "-J",
        "--no-warnings",
        "--playlist-end",
        &end,
        "--",
        url,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let out = runner.run(&cfg.binary, &args, Duration::from_secs(60))?;
    if !out.success {
        return Err(ResolveError::from_stderr(&out.stderr));
    }
    parse_page(&out.stdout, url)
}

fn parse(json: &str) -> Result<Vec<Hit>, ResolveError> {
    let flat: Flat =
        serde_json::from_str(json).map_err(|e| ResolveError::BadOutput(e.to_string()))?;
    Ok(flat.entries.into_iter().filter_map(hit).collect())
}

fn parse_page(json: &str, url: &str) -> Result<Vec<Hit>, ResolveError> {
    let page: Page =
        serde_json::from_str(json).map_err(|e| ResolveError::BadOutput(e.to_string()))?;
    Ok(match page.entries {
        Some(entries) => entries
            .into_iter()
            .filter(|e| {
                !matches!(
                    e.title.as_deref(),
                    Some("[Private video]" | "[Deleted video]")
                )
            })
            .filter_map(hit)
            .collect(),
        // Single video: its top-level `url` may be a direct media URL, not the page.
        None => {
            let page_url = page.webpage_url.unwrap_or_else(|| url.to_string());
            let info = Entry {
                url: Some(page_url.clone()),
                ..page.info
            };
            hit(info)
                .map(|h| Hit { url: page_url, ..h })
                .into_iter()
                .collect()
        }
    })
}

fn hit(e: Entry) -> Option<Hit> {
    let url = match (&e.url, &e.id) {
        (Some(u), _) if u.starts_with("https://") => u.clone(),
        (_, Some(id)) => format!("https://www.youtube.com/watch?v={id}"),
        _ => return None,
    };
    // Mid-sized thumbnail: the largest under ~500 px wide, else the last one.
    let thumb = e
        .thumbnails
        .iter()
        .rfind(|t| t.width.is_some_and(|w| w <= 500))
        .or(e.thumbnails.last())
        .and_then(|t| t.url.clone());
    Some(Hit {
        url,
        title: e.title.unwrap_or_default(),
        channel: e.channel.or(e.uploader).unwrap_or_default(),
        duration_secs: e.duration,
        thumbnail: thumb,
        live: matches!(e.live_status.as_deref(), Some("is_live" | "is_upcoming")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::Output;
    use std::cell::RefCell;

    struct Fake(String, RefCell<Vec<String>>);
    impl CommandRunner for Fake {
        fn run(&self, _: &str, args: &[String], _: Duration) -> Result<Output, ResolveError> {
            *self.1.borrow_mut() = args.to_vec();
            Ok(Output {
                success: true,
                stdout: self.0.clone(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn parses_flat_search() {
        let json = r#"{"entries":[
          {"id":"abc","url":"https://www.youtube.com/watch?v=abc","title":"Trailer","channel":"Ch","duration":125.0,
           "thumbnails":[{"url":"https://i/s.jpg","width":168},{"url":"https://i/m.jpg","width":336},{"url":"https://i/l.jpg","width":1280}]},
          {"id":"live1","title":"Live","uploader":"U","live_status":"is_live","thumbnails":[]},
          {"title":"no id"}]}"#;
        let f = Fake(json.into(), RefCell::new(vec![]));
        let hits = search("  dune  ", 50, &YtDlpConfig::default(), &f).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].thumbnail.as_deref(), Some("https://i/m.jpg"));
        assert_eq!(hits[0].channel, "Ch");
        assert_eq!(hits[1].url, "https://www.youtube.com/watch?v=live1");
        assert!(hits[1].live);
        let args = f.1.borrow();
        assert_eq!(args.last().unwrap(), "ytsearch20:dune");
        assert_eq!(args[args.len() - 2], "--");
    }

    #[test]
    fn empty_query_runs_nothing() {
        let f = Fake(String::new(), RefCell::new(vec![]));
        assert!(search("   ", 10, &YtDlpConfig::default(), &f)
            .unwrap()
            .is_empty());
        assert!(f.1.borrow().is_empty());
    }

    #[test]
    fn playlist_lists_entries_and_skips_unavailable() {
        let json = r#"{"_type":"playlist","title":"Mix","webpage_url":"https://www.youtube.com/playlist?list=PL1",
          "entries":[{"id":"a1","url":"https://www.youtube.com/watch?v=a1","title":"One","duration":200.0},
                     {"id":"p1","url":"https://www.youtube.com/watch?v=p1","title":"[Private video]"},
                     {"id":"b2","title":"Two","channel":"Band"}]}"#;
        let f = Fake(json.into(), RefCell::new(vec![]));
        let url = "https://www.youtube.com/playlist?list=PL1";
        let hits = playlist(url, &YtDlpConfig::default(), &f).unwrap();
        let titles: Vec<_> = hits.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(titles, ["One", "Two"]);
        assert_eq!(hits[1].url, "https://www.youtube.com/watch?v=b2");
        let args = f.1.borrow();
        assert_eq!(args.last().unwrap(), url);
        assert_eq!(args[args.len() - 2], "--");
    }

    #[test]
    fn playlist_of_single_video_is_the_page() {
        let json = r#"{"_type":"video","id":"v9","title":"Song","duration":181.5,"channel":"Artist",
          "url":"https://rr1.googlevideo.com/videoplayback?x","webpage_url":"https://www.youtube.com/watch?v=v9"}"#;
        let f = Fake(json.into(), RefCell::new(vec![]));
        let hits = playlist("https://youtu.be/v9", &YtDlpConfig::default(), &f).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].url, "https://www.youtube.com/watch?v=v9");
        assert_eq!(hits[0].title, "Song");
        assert_eq!(hits[0].duration_secs, Some(181.5));
    }
}
