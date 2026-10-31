use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;
use url::Url;

use crate::classify::{classify, UrlKind};
use crate::runner::{CommandRunner, ProcessRunner};
use crate::ResolveError;

/// Same format selector the plan proposes for the room (docs/design/client.md §3.7).
/// Prefer direct http(s) byte-range streams over HLS (m3u8) segment manifests so mpv can
/// buffer ahead smoothly when muxing video + audio.
pub const DEFAULT_FORMAT: &str = "bv*[height<=1080][protocol^=http][protocol!*=m3u8]+ba[protocol^=http][protocol!*=m3u8]/bv*[height<=1080]+ba/b";

#[derive(Debug, Clone)]
pub struct YtDlpConfig {
    /// Executable name or path (sidecar path in the packaged app).
    pub binary: String,
    pub format: String,
    pub timeout: Duration,
}

impl Default for YtDlpConfig {
    fn default() -> Self {
        Self {
            binary: "yt-dlp".into(),
            format: DEFAULT_FORMAT.into(),
            timeout: Duration::from_secs(30),
        }
    }
}

/// Everything mpv needs to start playback.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedStream {
    /// Video (or combined) URL.
    pub video_url: String,
    /// Separate audio URL, to pass to mpv as `--audio-file`.
    pub audio_url: Option<String>,
    pub title: String,
    pub duration_secs: Option<f64>,
    /// Other qualities (one per height, best first). Includes the chosen `video_url`.
    pub videos: Vec<Alt>,
    /// Audio languages (dubs) when the site offers more than one. Includes `audio_url`.
    pub audios: Vec<Alt>,
    /// Subtitles: uploaded ones, plus YouTube's automatic Turkish when no Turkish exists.
    pub subs: Vec<Sub>,
    /// Cover image (`thumbnail`), shown for audio-only streams.
    pub thumbnail: Option<String>,
}

/// An alternative track the player can switch to (`video-add` / `audio-add`).
#[derive(Debug, Clone, PartialEq)]
pub struct Alt {
    pub label: String,
    pub url: String,
    /// Audio: language code (empty when unknown).
    pub lang: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sub {
    pub url: String,
    pub lang: String,
    pub title: String,
}

#[derive(Deserialize)]
struct Info {
    title: Option<String>,
    thumbnail: Option<String>,
    duration: Option<f64>,
    is_live: Option<bool>,
    live_status: Option<String>,
    url: Option<String>,
    requested_formats: Option<Vec<Fmt>>,
    #[serde(default)]
    formats: Vec<Fmt>,
    #[serde(default)]
    subtitles: HashMap<String, Vec<SubFmt>>,
    #[serde(default)]
    automatic_captions: HashMap<String, Vec<SubFmt>>,
}

#[derive(Deserialize, Clone)]
struct Fmt {
    url: Option<String>,
    vcodec: Option<String>,
    acodec: Option<String>,
    height: Option<u32>,
    tbr: Option<f64>,
    protocol: Option<String>,
    language: Option<String>,
    language_preference: Option<i32>,
    format_note: Option<String>,
}

#[derive(Deserialize)]
struct SubFmt {
    url: Option<String>,
    ext: Option<String>,
    name: Option<String>,
}

fn has(codec: &Option<String>) -> bool {
    codec.as_deref().is_some_and(|c| c != "none")
}

/// Classify and resolve with the real `yt-dlp` using default settings.
pub fn resolve(input: &str) -> Result<ResolvedStream, ResolveError> {
    resolve_with(input, &YtDlpConfig::default(), &ProcessRunner)
}

pub fn resolve_with(
    input: &str,
    cfg: &YtDlpConfig,
    runner: &dyn CommandRunner,
) -> Result<ResolvedStream, ResolveError> {
    match classify(input)? {
        UrlKind::Direct(url) => Ok(direct(&url)),
        UrlKind::YtDlp(url) => via_ytdlp(&url, cfg, runner),
    }
}

fn direct(url: &Url) -> ResolvedStream {
    let title = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| url.host_str().unwrap_or("video"))
        .to_owned();
    ResolvedStream {
        video_url: url.to_string(),
        audio_url: None,
        title,
        duration_secs: None,
        videos: Vec::new(),
        audios: Vec::new(),
        subs: Vec::new(),
        thumbnail: None,
    }
}

fn via_ytdlp(
    url: &Url,
    cfg: &YtDlpConfig,
    runner: &dyn CommandRunner,
) -> Result<ResolvedStream, ResolveError> {
    let args: Vec<String> = [
        "-J",
        "--no-playlist",
        "--no-warnings",
        "-f",
        &cfg.format,
        "--",
        url.as_str(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let out = runner.run(&cfg.binary, &args, cfg.timeout)?;
    if !out.success {
        if out.stderr.contains("Unsupported URL") {
            return Err(ResolveError::Unsupported(
                url.host_str().unwrap_or_default().to_owned(),
            ));
        }
        return Err(ResolveError::from_stderr(&out.stderr));
    }
    parse_info(&out.stdout)
}

fn parse_info(json: &str) -> Result<ResolvedStream, ResolveError> {
    let info: Info =
        serde_json::from_str(json).map_err(|e| ResolveError::BadOutput(e.to_string()))?;

    if info.is_live == Some(true)
        || matches!(info.live_status.as_deref(), Some("is_live" | "is_upcoming"))
    {
        return Err(ResolveError::LiveUnsupported);
    }

    let title = info.title.unwrap_or_else(|| "Başlıksız".into());
    let (video_url, audio_url) = match info.requested_formats.as_deref() {
        Some(fmts) if !fmts.is_empty() => {
            let v_fmt = fmts
                .iter()
                .find(|f| has(&f.vcodec))
                .or_else(|| fmts.first());
            let video = match v_fmt {
                Some(vf) if !playable(vf) => info
                    .formats
                    .iter()
                    .filter(|f| has(&f.vcodec) && playable(f) && f.height.unwrap_or(0) <= 1080)
                    .max_by_key(|f| {
                        (
                            f.height.unwrap_or(0),
                            (f.tbr.unwrap_or(0.0) * 1000.0) as i64,
                        )
                    })
                    .and_then(|f| f.url.clone())
                    .or_else(|| vf.url.clone()),
                Some(vf) => vf.url.clone(),
                None => None,
            };
            // Separate audio only when it is a distinct audio-only track.
            let a_fmt = fmts.iter().find(|f| has(&f.acodec) && !has(&f.vcodec));
            let audio = match a_fmt {
                Some(af) if !playable(af) => info
                    .formats
                    .iter()
                    .filter(|f| has(&f.acodec) && !has(&f.vcodec) && playable(f))
                    .max_by_key(|f| {
                        (
                            f.language_preference.unwrap_or(-1),
                            (f.tbr.unwrap_or(0.0) * 1000.0) as i64,
                        )
                    })
                    .and_then(|f| f.url.clone())
                    .or_else(|| af.url.clone()),
                Some(af) => af.url.clone(),
                None => None,
            };
            (video, audio)
        }
        _ => (info.url, None),
    };

    let video_url = video_url.ok_or_else(|| ResolveError::BadOutput("akış URL'si yok".into()))?;
    let chosen = |u: &Option<String>| info.formats.iter().find(|f| f.url == *u).cloned();
    let videos = video_alts(&info.formats, chosen(&Some(video_url.clone())));
    let audios = audio_alts(
        &info.formats,
        audio_url.as_ref().and_then(|a| chosen(&Some(a.clone()))),
    );
    let subs = subtitles(&info.subtitles, &info.automatic_captions);
    Ok(ResolvedStream {
        video_url,
        audio_url,
        title,
        duration_secs: info.duration,
        videos,
        audios,
        subs,
        thumbnail: info
            .thumbnail
            .filter(|t| t.starts_with("https://") || t.starts_with("http://")),
    })
}

/// mpv can open plain http(s) formats; manifests/segment lists are skipped.
fn playable(f: &Fmt) -> bool {
    f.url.is_some() && matches!(f.protocol.as_deref(), None | Some("https" | "http"))
}

/// One format per height (video-only first, AV1 last: slow to decode without a new GPU,
/// then highest bitrate); `chosen` keeps its height's slot. Empty if only one height.
fn video_alts(formats: &[Fmt], chosen: Option<Fmt>) -> Vec<Alt> {
    let rank = |f: &Fmt| {
        (
            !has(&f.acodec),
            !f.vcodec.as_deref().unwrap_or("").starts_with("av01"),
            (f.tbr.unwrap_or(0.0) * 1000.0) as i64,
        )
    };
    let mut best: Vec<&Fmt> = Vec::new();
    for f in formats
        .iter()
        .filter(|f| has(&f.vcodec) && playable(f) && f.height.is_some())
    {
        match best.iter_mut().find(|b| b.height == f.height) {
            Some(b) if rank(f) > rank(b) => *b = f,
            Some(_) => {}
            None => best.push(f),
        }
    }
    if let Some(c) = &chosen {
        if let Some(b) = best.iter_mut().find(|b| b.height == c.height) {
            *b = c;
        }
    }
    best.sort_by_key(|f| std::cmp::Reverse(f.height));
    if best.len() < 2 {
        return Vec::new();
    }
    best.into_iter()
        .map(|f| Alt {
            label: format!("{}p", f.height.unwrap_or(0)),
            url: f.url.clone().unwrap_or_default(),
            lang: String::new(),
        })
        .collect()
}

/// Best audio-only format per language (original first). Empty if only one language.
fn audio_alts(formats: &[Fmt], chosen: Option<Fmt>) -> Vec<Alt> {
    let mut best: Vec<&Fmt> = Vec::new();
    for f in formats
        .iter()
        .filter(|f| has(&f.acodec) && !has(&f.vcodec) && playable(f) && f.language.is_some())
    {
        match best.iter_mut().find(|b| b.language == f.language) {
            Some(b) if f.tbr.unwrap_or(0.0) > b.tbr.unwrap_or(0.0) => *b = f,
            Some(_) => {}
            None => best.push(f),
        }
    }
    if let Some(c) = &chosen {
        if let Some(b) = best.iter_mut().find(|b| b.language == c.language) {
            *b = c;
        }
    }
    if best.len() < 2 {
        return Vec::new();
    }
    best.sort_by_key(|f| std::cmp::Reverse(f.language_preference.unwrap_or(-1)));
    best.into_iter()
        .map(|f| {
            let lang = f.language.clone().unwrap_or_default();
            // YouTube: "Turkish (dubbed-auto), medium" -> "Turkish (dubbed-auto)"
            let note = f.format_note.as_deref().unwrap_or("");
            let name = note.split(',').next().unwrap_or("").trim();
            let label = if name.is_empty() || name.contains("kbps") {
                lang.clone()
            } else {
                name.to_owned()
            };
            Alt {
                label,
                url: f.url.clone().unwrap_or_default(),
                lang,
            }
        })
        .collect()
}

const MAX_SUBS: usize = 12;

fn sub_url(list: &[SubFmt]) -> Option<(&SubFmt, String)> {
    ["vtt", "srt", "ass"]
        .iter()
        .find_map(|e| list.iter().find(|s| s.ext.as_deref() == Some(e)))
        .and_then(|s| s.url.clone().map(|u| (s, u)))
}

/// Uploaded subtitles (Turkish and English first), plus YouTube's automatic Turkish
/// translation when no uploaded Turkish exists.
fn subtitles(
    manual: &HashMap<String, Vec<SubFmt>>,
    auto: &HashMap<String, Vec<SubFmt>>,
) -> Vec<Sub> {
    let order = |l: &str| match l.split('-').next().unwrap_or(l) {
        "tr" => 0,
        "en" => 1,
        _ => 2,
    };
    let mut langs: Vec<&String> = manual.keys().filter(|l| *l != "live_chat").collect();
    langs.sort_by_key(|l| (order(l), l.to_string()));
    let mut out: Vec<Sub> = langs
        .into_iter()
        .filter_map(|l| {
            let (s, url) = sub_url(&manual[l])?;
            Some(Sub {
                url,
                lang: l.clone(),
                title: s.name.clone().unwrap_or_else(|| l.clone()),
            })
        })
        .take(MAX_SUBS)
        .collect();
    if !out.iter().any(|s| order(&s.lang) == 0) {
        if let Some((_, url)) = auto.get("tr").and_then(|l| sub_url(l)) {
            out.push(Sub {
                url,
                lang: "tr".into(),
                title: "Türkçe (otomatik)".into(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::Output;
    use std::cell::RefCell;

    struct Fake {
        out: Result<Output, ResolveError>,
        seen: RefCell<Vec<String>>,
    }
    impl Fake {
        fn ok(stdout: &str) -> Self {
            Self {
                out: Ok(Output {
                    success: true,
                    stdout: stdout.into(),
                    stderr: String::new(),
                }),
                seen: Default::default(),
            }
        }
        fn fail(stderr: &str) -> Self {
            Self {
                out: Ok(Output {
                    success: false,
                    stdout: String::new(),
                    stderr: stderr.into(),
                }),
                seen: Default::default(),
            }
        }
    }
    impl CommandRunner for Fake {
        fn run(&self, _: &str, args: &[String], _: Duration) -> Result<Output, ResolveError> {
            *self.seen.borrow_mut() = args.to_vec();
            match &self.out {
                Ok(o) => Ok(o.clone()),
                Err(ResolveError::YtDlpNotFound(p)) => Err(ResolveError::YtDlpNotFound(p.clone())),
                Err(_) => unreachable!(),
            }
        }
    }

    const YT: &str = "https://youtu.be/dQw4w9WgXcQ";

    #[test]
    fn split_video_audio() {
        let json = r#"{"title":"Şarkı","duration":212.0,"requested_formats":[
            {"url":"https://v/video","vcodec":"avc1","acodec":"none"},
            {"url":"https://v/audio","vcodec":"none","acodec":"mp4a"}]}"#;
        let f = Fake::ok(json);
        let r = resolve_with(YT, &YtDlpConfig::default(), &f).unwrap();
        assert_eq!(r.video_url, "https://v/video");
        assert_eq!(r.audio_url.as_deref(), Some("https://v/audio"));
        assert_eq!(r.title, "Şarkı");
        assert_eq!(r.duration_secs, Some(212.0));
        let args = f.seen.borrow();
        assert!(args.contains(&"-J".to_string()));
        assert!(args.contains(&"--no-playlist".to_string()));
        assert_eq!(args.last().unwrap(), YT);
        assert_eq!(args[args.len() - 2], "--"); // URL can never be parsed as a flag
    }

    #[test]
    fn single_combined_url() {
        let f = Fake::ok(r#"{"title":"T","duration":10,"url":"https://v/combined"}"#);
        let r = resolve_with(YT, &YtDlpConfig::default(), &f).unwrap();
        assert_eq!(r.video_url, "https://v/combined");
        assert_eq!(r.audio_url, None);
    }

    #[test]
    fn combined_requested_format_has_no_separate_audio() {
        let f = Fake::ok(
            r#"{"title":"T","requested_formats":[{"url":"https://v/c","vcodec":"avc1","acodec":"mp4a"}]}"#,
        );
        let r = resolve_with(YT, &YtDlpConfig::default(), &f).unwrap();
        assert_eq!(r.video_url, "https://v/c");
        assert_eq!(r.audio_url, None);
    }

    #[test]
    fn live_is_rejected() {
        for j in [
            r#"{"title":"T","is_live":true,"url":"u"}"#,
            r#"{"title":"T","live_status":"is_upcoming","url":"u"}"#,
        ] {
            let r = resolve_with(YT, &YtDlpConfig::default(), &Fake::ok(j));
            assert!(matches!(r, Err(ResolveError::LiveUnsupported)));
        }
    }

    type ErrCheck = fn(&ResolveError) -> bool;

    #[test]
    fn error_mapping() {
        let cases: [(&str, ErrCheck); 5] = [
            ("ERROR: [youtube] x: Private video. Sign in", |e| {
                matches!(e, ResolveError::Private)
            }),
            ("ERROR: [youtube] x: Video unavailable", |e| {
                matches!(e, ResolveError::Unavailable)
            }),
            (
                "ERROR: The uploader has not made this video available in your country",
                |e| matches!(e, ResolveError::GeoBlocked),
            ),
            ("ERROR: [youtube] x: Sign in to confirm your age", |e| {
                matches!(e, ResolveError::AgeRestricted)
            }),
            (
                "WARNING: foo\nERROR: something odd\n",
                |e| matches!(e, ResolveError::Failed(m) if m == "ERROR: something odd"),
            ),
        ];
        for (stderr, check) in cases {
            let e = resolve_with(YT, &YtDlpConfig::default(), &Fake::fail(stderr)).unwrap_err();
            assert!(check(&e), "{stderr} -> {e:?}");
        }
    }

    #[test]
    fn bad_json_and_missing_url() {
        let e = resolve_with(YT, &YtDlpConfig::default(), &Fake::ok("not json")).unwrap_err();
        assert!(matches!(e, ResolveError::BadOutput(_)));
        let e =
            resolve_with(YT, &YtDlpConfig::default(), &Fake::ok(r#"{"title":"T"}"#)).unwrap_err();
        assert!(matches!(e, ResolveError::BadOutput(_)));
    }

    #[test]
    fn binary_not_found_is_clean() {
        let cfg = YtDlpConfig {
            binary: "definitely-not-yt-dlp-xyz".into(),
            ..Default::default()
        };
        let e = resolve_with(YT, &cfg, &ProcessRunner).unwrap_err();
        assert!(matches!(e, ResolveError::YtDlpNotFound(_)));
    }

    #[test]
    fn timeout_kills_child() {
        let e = ProcessRunner
            .run("sleep", &["5".into()], Duration::from_millis(100))
            .unwrap_err();
        assert!(matches!(e, ResolveError::Timeout(_)));
    }

    #[test]
    fn direct_passthrough_does_not_spawn() {
        let f = Fake::fail("should not run");
        let r = resolve_with(
            "https://example.com/a/clip.mp4",
            &YtDlpConfig::default(),
            &f,
        )
        .unwrap();
        assert_eq!(r.video_url, "https://example.com/a/clip.mp4");
        assert_eq!(r.title, "clip.mp4");
        assert!(f.seen.borrow().is_empty());
    }

    #[test]
    fn qualities_dubs_and_subs() {
        let json = r#"{"title":"T","requested_formats":[
            {"url":"https://v/1080","vcodec":"vp9","acodec":"none","height":1080,"protocol":"https"},
            {"url":"https://a/en","vcodec":"none","acodec":"opus","language":"en","protocol":"https"}],
          "formats":[
            {"url":"https://v/360","vcodec":"avc1","acodec":"mp4a","height":360,"tbr":500,"protocol":"https"},
            {"url":"https://v/360v","vcodec":"avc1","acodec":"none","height":360,"tbr":300,"protocol":"https"},
            {"url":"https://v/720","vcodec":"avc1","acodec":"none","height":720,"tbr":2000,"protocol":"https"},
            {"url":"https://v/720hls","vcodec":"avc1","acodec":"none","height":720,"tbr":9000,"protocol":"m3u8_native"},
            {"url":"https://v/1080av1","vcodec":"av01","acodec":"none","height":1080,"tbr":9000,"protocol":"https"},
            {"url":"https://v/1080","vcodec":"vp9","acodec":"none","height":1080,"tbr":3000,"protocol":"https"},
            {"url":"https://a/tr","vcodec":"none","acodec":"opus","language":"tr","tbr":130,"format_note":"Turkish (dubbed-auto), medium","language_preference":-1,"protocol":"https"},
            {"url":"https://a/en","vcodec":"none","acodec":"opus","language":"en","tbr":130,"format_note":"English original (default), medium","language_preference":10,"protocol":"https"}],
          "subtitles":{"de":[{"ext":"json3","url":"https://s/de.json"},{"ext":"vtt","url":"https://s/de.vtt","name":"Deutsch"}],
                       "live_chat":[{"ext":"json","url":"x"}]},
          "automatic_captions":{"tr":[{"ext":"vtt","url":"https://s/tr-auto.vtt"}]}}"#;
        let r = resolve_with(YT, &YtDlpConfig::default(), &Fake::ok(json)).unwrap();
        let v: Vec<_> = r
            .videos
            .iter()
            .map(|a| (a.label.as_str(), a.url.as_str()))
            .collect();
        assert_eq!(
            v,
            [
                ("1080p", "https://v/1080"),
                ("720p", "https://v/720"),
                ("360p", "https://v/360v")
            ]
        );
        let a: Vec<_> = r
            .audios
            .iter()
            .map(|a| (a.label.as_str(), a.lang.as_str()))
            .collect();
        assert_eq!(
            a,
            [
                ("English original (default)", "en"),
                ("Turkish (dubbed-auto)", "tr")
            ]
        );
        let s: Vec<_> = r
            .subs
            .iter()
            .map(|s| (s.lang.as_str(), s.url.as_str()))
            .collect();
        assert_eq!(
            s,
            [("de", "https://s/de.vtt"), ("tr", "https://s/tr-auto.vtt")]
        );
    }

    #[test]
    fn single_quality_has_no_alternatives() {
        let f = Fake::ok(
            r#"{"title":"T","url":"https://v/c","formats":[{"url":"https://v/c","vcodec":"avc1","acodec":"mp4a","height":720}]}"#,
        );
        let r = resolve_with(YT, &YtDlpConfig::default(), &f).unwrap();
        assert!(r.videos.is_empty() && r.audios.is_empty() && r.subs.is_empty());
    }
}
