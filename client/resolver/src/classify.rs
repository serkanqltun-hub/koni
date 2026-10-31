use url::Url;

use crate::ResolveError;

const DIRECT_EXTS: &[&str] = &["mp4", "m3u8", "webm", "mkv", "mov"];

/// How a pasted URL should be handled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlKind {
    /// Plain media file or HLS playlist; mpv can open it as is.
    Direct(Url),
    /// Any other web page: yt-dlp extracts the real stream (YouTube and other VOD sites).
    YtDlp(Url),
}

/// Classify user input. Surrounding whitespace is ignored.
pub fn classify(input: &str) -> Result<UrlKind, ResolveError> {
    let url =
        Url::parse(input.trim()).map_err(|_| ResolveError::InvalidUrl(input.trim().to_owned()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ResolveError::InvalidUrl(input.trim().to_owned()));
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(ResolveError::InvalidUrl(input.trim().to_owned()));
    }

    // Extension of the last path segment; the query string is not part of the path.
    let ext = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .and_then(|seg| seg.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase());
    if ext.is_some_and(|e| DIRECT_EXTS.contains(&e.as_str())) {
        return Ok(UrlKind::Direct(url));
    }
    Ok(UrlKind::YtDlp(url))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(s: &str) -> Result<UrlKind, ResolveError> {
        classify(s)
    }

    #[test]
    fn youtube_variants() {
        for u in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "  https://youtu.be/dQw4w9WgXcQ\n",
        ] {
            assert!(matches!(kind(u), Ok(UrlKind::YtDlp(_))), "{u}");
        }
    }

    #[test]
    fn direct_links() {
        for u in [
            "https://example.com/a/video.mp4",
            "https://example.com/VIDEO.MP4",
            "https://cdn.example.com/live/master.m3u8?token=abc.def",
            "http://example.com/x.webm",
        ] {
            assert!(matches!(kind(u), Ok(UrlKind::Direct(_))), "{u}");
        }
    }

    #[test]
    fn extension_must_be_in_path_not_query() {
        assert!(matches!(
            kind("https://example.com/watch?file=a.mp4"),
            Ok(UrlKind::YtDlp(_))
        ));
    }

    #[test]
    fn other_sites_go_through_ytdlp() {
        assert!(matches!(
            kind("https://vimeo.com/12345"),
            Ok(UrlKind::YtDlp(_))
        ));
    }

    #[test]
    fn invalid_input() {
        for u in [
            "",
            "not a url",
            "ftp://example.com/a.mp4",
            "file:///tmp/a.mp4",
            "javascript:alert(1)",
        ] {
            assert!(matches!(kind(u), Err(ResolveError::InvalidUrl(_))), "{u}");
        }
    }
}
