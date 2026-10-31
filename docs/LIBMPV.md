# libmpv (LGPL) — Windows

[ADR 0006](adr/0006-libmpv-lgpl.md) requires libmpv and FFmpeg inside it to be **LGPL builds** (`mpv -Dgpl=false`,
FFmpeg without `--enable-gpl`/`--enable-nonfree`). Ready-made Windows builds (shinchiro,
zhongfly, MSYS2 packages) are GPL builds, so KONİ builds its own.

## Getting the DLL
The workflow "libmpv (LGPL, Windows x64)" runs **only manually** (Actions → *Run workflow*,
~20 min); pull requests and tags never trigger it.
* **CI artifact**: the run's `libmpv-lgpl-win64` artifact (optional `mpv_ref` / `ffmpeg_ref`).
* **Release**: fill the `release` input (e.g. `libmpv-v0.40.0-r2`) → the zip is published as that
  GitHub release. `.github/workflows/release.yml` (`LIBMPV_RELEASE`) downloads the app's libmpv from it.

Package contents: `libmpv-2.dll`, FFmpeg DLLs (`avcodec-*.dll`, …), dependency DLLs,
`NOTICE.md`, `LICENSES/`, `BUILDINFO.txt` (exact versions/commits), `source/` (mpv + FFmpeg
source tarballs + build script), `REBUILD.md`, `SHA256SUMS.txt`.

## Using it from the Tauri client
Place all DLLs next to the client executable (or in a `libmpv/` folder added to the DLL search
path) and load `libmpv-2.dll` dynamically. Never link statically and never pin hashes/signatures
of these DLLs (LGPL: user must be able to replace them). Ship `NOTICE.md`, `LICENSES/` and
`source/` with the installer (or an "About → Open-source licenses" screen plus a written source offer).
The Windows installer does exactly this: `client/scripts/build-installer.ps1 -Libmpv <artifact zip>`
copies the whole package next to `koni.exe` (see client/README). The build has no Lua, so
script-only options such as `osc` are set best-effort.

## What CI checks
`tools/libmpv/verify.sh`: no `--enable-gpl/nonfree/version3` in FFmpeg's embedded config, no
x264/x265/xvid/fdk strings, `mpv_*` exports present; then a smoke test creates and initializes an
mpv context. Features deliberately off: Lua/JS scripting, cplayer, all auto-detected extras.

## Open points
* Codecs are FFmpeg's native ones + dav1d (AV1). HEVC/H.264 decode via native decoders / D3D11VA.
* Patent licensing of codecs is a separate question from copyright licensing [doğrulanmalı].
* First CI artifact verified locally with the client (2026-10-29): local MKV, YouTube 360p and 1080p streams play, **but silently**: `-Dauto_features=disabled` also disabled the `wasapi` audio output (mpv log: `Could not open/initialize audio device -> no sound`). `build.sh` now enables `-Dwasapi=enabled` and fails if the DLL has no wasapi.

# libmpv (LGPL) — macOS (Apple Silicon)

Homebrew's mpv/FFmpeg are GPL builds, so the macOS package is built from source too:
`tools/libmpv/build-macos.sh` (workflow "libmpv (LGPL, macOS arm64)", ~25 min, manual; pull
requests that change the script also build it, without publishing). Every library (mpv,
FFmpeg, libplacebo, libass, HarfBuzz, FriBidi, FreeType, dav1d) is a shared dylib with an
`@rpath/<name>` install name and an `@loader_path` rpath, ad hoc signed.

* **Release**: fill the `release` input (e.g. `libmpv-macos-v0.40.0-r1`). `release.yml` and the
  macOS job of `client.yml` (`LIBMPV_MAC_RELEASE`) download it; until the release exists,
  `client.yml` takes the newest successful workflow artifact.
* **In the app**: `client/scripts/build-macos.sh` puts the dylibs into
  `KONI.app/Contents/Frameworks` and `NOTICE.md`, `LICENSES/`, `source/` into
  `Contents/Resources/libmpv/`. Users can swap the dylibs and re-sign ad hoc (`REBUILD.md`).
* **Rendering**: mpv can't embed into another app's view on macOS (`wid` is not supported
  there), so the app runs libmpv with `vo=libmpv` and draws through the render API into an
  OpenGL view (`client/crates/koni-surface/src/macos.rs`). Hence `-Dgl=enabled -Dgl-cocoa=enabled`
  and `-Dvideotoolbox-gl=enabled` for hardware decoding; `coreaudio` for sound.
* **CI checks**: FFmpeg config without GPL/nonfree, no GPL codec strings, `mpv_*` and
  `mpv_render_context_*` exports, every dylib links only `@rpath` or the OS, then the
  koni-player smoke test (and in `client.yml` also `--render-sw`, a frame through the render API).
