# Third-party notice — libmpv (KONİ)

KONİ uses **libmpv** as a dynamically loaded library (`libmpv-2.dll`). This package
contains an unmodified build of:

| Component | License | Source |
|---|---|---|
| mpv / libmpv (built with `-Dgpl=false`) | LGPL-2.1-or-later | https://github.com/mpv-player/mpv |
| FFmpeg (libavcodec, libavformat, libavutil, libswresample, libswscale; no `--enable-gpl`, no `--enable-nonfree`) | LGPL-2.1-or-later | https://ffmpeg.org |
| libplacebo | LGPL-2.1-or-later | https://code.videolan.org/videolan/libplacebo |
| dav1d | BSD-2-Clause | https://code.videolan.org/videolan/dav1d |
| libass | ISC | https://github.com/libass/libass |
| FreeType (FTL option), HarfBuzz, FriBidi, zlib, lcms2, shaderc, MinGW-w64 runtime | see `LICENSES/third-party/` | MSYS2 packages |

Full license texts: `LICENSES/`.
Exact versions and commits: `BUILDINFO.txt`.

## Your rights under the LGPL

* **Source code**: the exact mpv and FFmpeg sources used, plus the build script, are in `source/`.
  Sources of the remaining libraries are available from the MSYS2 package repository
  (`https://packages.msys2.org`) for the versions listed in `BUILDINFO.txt`.
* **Replacing the libraries**: all LGPL components are separate DLLs, loaded dynamically.
  You may replace `libmpv-2.dll`, `avcodec-*.dll`, etc. with your own compatible build
  (see `REBUILD.md`). KONİ does not verify signatures or hashes of these DLLs at load time.
* KONİ's own code is not covered by the LGPL.
