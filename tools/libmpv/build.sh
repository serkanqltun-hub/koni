#!/usr/bin/env bash
# KONİ — LGPL libmpv-2.dll build (MSYS2 MINGW64 shell).
# Usage: tools/libmpv/build.sh <out-dir>
# Env:   MPV_REF (default v0.40.0), FFMPEG_REF (default n7.1.1)
set -euo pipefail

OUT="$(realpath -m "${1:?usage: build.sh <out-dir>}")"
MPV_REF="${MPV_REF:-v0.40.0}"
FFMPEG_REF="${FFMPEG_REF:-n7.1.1}"
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="${WORK:-$(pwd)/_libmpv_work}"
PREFIX="$WORK/prefix"
JOBS="$(nproc)"

mkdir -p "$WORK" "$PREFIX" "$OUT"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig:${PKG_CONFIG_PATH:-}"

# --- sources (also shipped in the artifact: LGPL source offer) -------------
fetch() { # name url ref
  [ -d "$WORK/$1" ] || git clone --depth 1 --branch "$3" "$2" "$WORK/$1"
  git -C "$WORK/$1" rev-parse HEAD > "$WORK/$1.rev"
}
fetch ffmpeg https://git.ffmpeg.org/ffmpeg.git "$FFMPEG_REF"
fetch mpv https://github.com/mpv-player/mpv.git "$MPV_REF"

# --- FFmpeg: LGPL only (no --enable-gpl / --enable-nonfree / GPL libs) ------
# Shared build on purpose: users must be able to swap the DLLs (LGPL §6).
cd "$WORK/ffmpeg"
./configure \
  --prefix="$PREFIX" --arch=x86_64 --target-os=mingw32 \
  --enable-shared --disable-static \
  --disable-autodetect --disable-programs --disable-doc --disable-debug \
  --disable-gpl --disable-nonfree --disable-version3 \
  --enable-d3d11va --enable-dxva2 --enable-schannel \
  --enable-libdav1d --enable-zlib \
  --extra-cflags="-O2"
make -j"$JOBS"
make install

# Hard guard: refuse to continue if anything GPL slipped in.
if grep -E '^#define (CONFIG_GPL|CONFIG_NONFREE|CONFIG_VERSION3) 1' config.h; then
  echo "ERROR: FFmpeg was configured with GPL/nonfree options" >&2; exit 1
fi

# --- mpv: -Dgpl=false, libmpv only ------------------------------------------
# auto_features=disabled turns off everything not listed here, including audio
# outputs: wasapi must be enabled explicitly or libmpv plays without sound.
cd "$WORK/mpv"
meson setup build --prefix="$PREFIX" --buildtype=release \
  -Dgpl=false -Dlibmpv=true -Dcplayer=false -Dtests=false \
  -Dauto_features=disabled \
  -Dzlib=enabled -Diconv=enabled -Dlcms2=enabled \
  -Dd3d11=enabled -Dd3d-hwaccel=enabled -Dd3d9-hwaccel=enabled \
  -Dshaderc=enabled -Dspirv-cross=enabled -Dwin32-threads=enabled \
  -Dwasapi=enabled \
  -Dgl=disabled -Dlua=disabled -Djavascript=disabled \
  --wrap-mode=nodownload
meson compile -C build
meson install -C build

# --- collect: libmpv + every non-system DLL it needs -------------------------
DLL="$(find "$PREFIX" -name 'libmpv-2.dll' | head -n1)"
[ -n "$DLL" ] || { echo "libmpv-2.dll not found" >&2; exit 1; }
# Hard guard: an mpv without an audio output plays silently.
grep -aq wasapi "$DLL" || { echo "ERROR: libmpv-2.dll has no wasapi audio output" >&2; exit 1; }
PKG="$OUT/libmpv-lgpl-win64"
rm -rf "$PKG"; mkdir -p "$PKG/LICENSES" "$PKG/source"
cp "$DLL" "$PKG/"
cp "$PREFIX"/bin/av*.dll "$PREFIX"/bin/sw*.dll "$PKG/"

# Pull in mingw runtime/dependency DLLs (ldd lists only what is really needed).
for _ in 1 2 3; do
  for f in "$PKG"/*.dll; do
    ldd "$f" | awk '/=> \/mingw64\// {print $3}' | while read -r dep; do
      [ -f "$PKG/$(basename "$dep")" ] || cp "$dep" "$PKG/"
    done
  done
done

# --- licence / attribution / source offer ------------------------------------
cp "$HERE/NOTICE.md" "$PKG/NOTICE.md"
cp "$WORK/mpv/LICENSE.LGPL" "$PKG/LICENSES/mpv-LGPL-2.1.txt"
cp "$WORK/ffmpeg/COPYING.LGPLv2.1" "$PKG/LICENSES/ffmpeg-LGPL-2.1.txt"
cp "$HERE/REBUILD.md" "$PKG/REBUILD.md"
for d in /mingw64/share/licenses/*; do
  [ -d "$d" ] && mkdir -p "$PKG/LICENSES/third-party" && cp -r "$d" "$PKG/LICENSES/third-party/"
done
git -C "$WORK/mpv"    archive --format=tar.gz -o "$PKG/source/mpv-$MPV_REF.tar.gz"       HEAD
git -C "$WORK/ffmpeg" archive --format=tar.gz -o "$PKG/source/ffmpeg-$FFMPEG_REF.tar.gz" HEAD
cp "$HERE/build.sh" "$PKG/source/build.sh"

{
  echo "mpv     $MPV_REF  $(cat "$WORK/mpv.rev")"
  echo "ffmpeg  $FFMPEG_REF  $(cat "$WORK/ffmpeg.rev")"
  echo "built   $(date -u +%FT%TZ)"
  echo "mpv option: -Dgpl=false ; FFmpeg: no --enable-gpl/--enable-nonfree/--enable-version3"
  pacman -Q 2>/dev/null | grep -E 'mingw-w64-x86_64-(libass|libplacebo|dav1d|freetype|harfbuzz|fribidi|zlib|lcms2|shaderc|spirv-cross)' || true
} > "$PKG/BUILDINFO.txt"

(cd "$PKG" && sha256sum *.dll > SHA256SUMS.txt)
echo "Done: $PKG"
