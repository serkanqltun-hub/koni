# Third-party notice — libmpv for macOS (KONİ)

KONİ uses **libmpv** as a dynamically loaded library (`libmpv.2.dylib`, inside
`KONI.app/Contents/Frameworks`). This package contains an unmodified build of:

| Component | License | Source |
|---|---|---|
| mpv / libmpv (built with `-Dgpl=false`) | LGPL-2.1-or-later | https://github.com/mpv-player/mpv |
| FFmpeg (libavcodec, libavformat, libavutil, libavfilter, libavdevice, libswresample, libswscale; no `--enable-gpl`, no `--enable-nonfree`) | LGPL-2.1-or-later | https://ffmpeg.org |
| libplacebo | LGPL-2.1-or-later | https://code.videolan.org/videolan/libplacebo |
| FriBidi | LGPL-2.1-or-later | https://github.com/fribidi/fribidi |
| dav1d | BSD-2-Clause | https://code.videolan.org/videolan/dav1d |
| libass | ISC | https://github.com/libass/libass |
| HarfBuzz | MIT | https://github.com/harfbuzz/harfbuzz |
| FreeType (FTL option) | FTL | https://freetype.org |

Full license texts: `LICENSES/`. Exact versions and commits: `BUILDINFO.txt`.

## Your rights under the LGPL

* **Source code**: the exact sources of every library above, plus the build script, are in `source/`.
* **Replacing the libraries**: every component is a separate dylib, loaded dynamically
  (`@rpath`, resolved next to `libmpv.2.dylib`). You may replace any of them with your own
  compatible build (see `REBUILD.md`). KONİ does not verify signatures or hashes of these
  libraries at load time.
* KONİ's own code is not covered by the LGPL.
