# Third-party notices

KONİ's own code is MIT licensed (see `LICENSE`). The Windows installer also ships or uses:

| Component | License | Source | Notes |
|---|---|---|---|
| mpv / libmpv, FFmpeg and friends | LGPL-2.1-or-later and others | see `tools/libmpv/NOTICE.md` (macOS: `NOTICE-macos.md`) | Separate, replaceable DLLs / dylibs; sources and licenses ship in the package. |
| yt-dlp | Unlicense | https://github.com/yt-dlp/yt-dlp | Unmodified `yt-dlp.exe`, run as a separate program. |
| uBlock Origin Lite | GPL-3.0 | https://github.com/uBlockOrigin/uBOL-home | Unmodified except `manifest.json`: `host_permissions` is set to `<all_urls>` so it runs without a permission prompt. Its own license file ships in `extensions/ubol/`. |

Data services (used through their public APIs; KONİ is not endorsed by any of them):

- **TMDB**: movie and series info, posters, trailers. This product uses the TMDB API but is not endorsed or certified by TMDB.
- **OMDb API**: ratings.
- **OpenSubtitles.com**: subtitle search.
- **LRCLIB**: song lyrics.
- **Cinemeta** (Stremio): title search and episode lists.
