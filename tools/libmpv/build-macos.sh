#!/usr/bin/env bash
# KONİ — LGPL libmpv.2.dylib build for macOS (Apple Silicon).
# Usage: tools/libmpv/build-macos.sh <out-dir>
# Env:   MPV_REF (default v0.40.0), FFMPEG_REF (default n7.1.1)
# Needs: Xcode command line tools + `brew install meson ninja nasm pkg-config`.
#
# Every library is built from source as a shared dylib (no Homebrew libraries end up
# in the package). Install names are rewritten to @rpath/<name> with an
# @loader_path rpath, so the folder works wherever it is copied (the app puts it in
# KONI.app/Contents/Frameworks) and each dylib can be replaced by the user (LGPL §6).
set -euo pipefail

OUT="$(mkdir -p "${1:?usage: build-macos.sh <out-dir>}" && cd "$1" && pwd)"
MPV_REF="${MPV_REF:-v0.40.0}"
FFMPEG_REF="${FFMPEG_REF:-n7.1.1}"
FREETYPE_REF="${FREETYPE_REF:-VER-2-13-3}"
FRIBIDI_REF="${FRIBIDI_REF:-v1.0.16}"
HARFBUZZ_REF="${HARFBUZZ_REF:-11.2.1}"
LIBASS_REF="${LIBASS_REF:-0.17.3}"
DAV1D_REF="${DAV1D_REF:-1.5.1}"
PLACEBO_REF="${PLACEBO_REF:-v7.349.0}"
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="${WORK:-$(pwd)/_libmpv_work}"
PREFIX="$WORK/prefix"
JOBS="$(sysctl -n hw.ncpu)"

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig"
export PATH="$PREFIX/bin:$PATH"
mkdir -p "$WORK" "$PREFIX" "$OUT"

fetch() { # name url ref [--recursive]
  [ -d "$WORK/$1" ] || git clone --depth 1 ${4:-} --branch "$3" "$2" "$WORK/$1"
  git -C "$WORK/$1" rev-parse HEAD > "$WORK/$1.rev"
}
fetch freetype https://github.com/freetype/freetype.git "$FREETYPE_REF"
fetch fribidi  https://github.com/fribidi/fribidi.git "$FRIBIDI_REF"
fetch harfbuzz https://github.com/harfbuzz/harfbuzz.git "$HARFBUZZ_REF"
fetch libass   https://github.com/libass/libass.git "$LIBASS_REF"
fetch dav1d    https://github.com/videolan/dav1d.git "$DAV1D_REF"
fetch libplacebo https://github.com/haasn/libplacebo.git "$PLACEBO_REF" --recursive
fetch ffmpeg   https://git.ffmpeg.org/ffmpeg.git "$FFMPEG_REF"
fetch mpv      https://github.com/mpv-player/mpv.git "$MPV_REF"

mesonlib() { # dir [meson options...]
  local d="$1"; shift
  (cd "$WORK/$d" && rm -rf build &&
    meson setup build --prefix="$PREFIX" --libdir=lib --buildtype=release \
      --default-library=shared --wrap-mode=nodownload "$@" &&
    meson compile -C build && meson install -C build)
}

# --- subtitle stack (libass + its font/shaping libraries) -------------------
mesonlib freetype -Dbrotli=disabled -Dbzip2=disabled -Dharfbuzz=disabled \
  -Dpng=disabled -Dzlib=system -Dtests=disabled
mesonlib fribidi -Ddocs=false -Dbin=false -Dtests=false
mesonlib harfbuzz -Dfreetype=enabled -Dglib=disabled -Dgobject=disabled \
  -Dcairo=disabled -Dicu=disabled -Dgraphite2=disabled -Dchafa=disabled \
  -Dtests=disabled -Ddocs=disabled -Dutilities=disabled -Dintrospection=disabled
mesonlib libass -Dfontconfig=disabled -Dcoretext=enabled -Dlibunibreak=disabled \
  -Ddirectwrite=disabled -Dtest=false -Dprofile=false

# --- decoders / renderer ----------------------------------------------------
mesonlib dav1d -Denable_tools=false -Denable_tests=false -Denable_examples=false
# OpenGL only: KONİ draws through libmpv's render API into an OpenGL view.
mesonlib libplacebo -Dvulkan=disabled -Dopengl=enabled -Dd3d11=disabled \
  -Dglslang=disabled -Dshaderc=disabled -Dlcms=disabled -Ddovi=disabled \
  -Dlibdovi=disabled -Dunwind=disabled -Dxxhash=disabled \
  -Ddemos=false -Dtests=false -Dbench=false -Dfuzz=false

# --- FFmpeg: LGPL only (no --enable-gpl / --enable-nonfree / GPL libs) ------
cd "$WORK/ffmpeg"
./configure \
  --prefix="$PREFIX" --install-name-dir='@rpath' \
  --enable-shared --disable-static \
  --disable-autodetect --disable-programs --disable-doc --disable-debug \
  --disable-gpl --disable-nonfree --disable-version3 \
  --enable-pthreads --enable-videotoolbox --enable-audiotoolbox \
  --enable-securetransport --enable-zlib --enable-iconv --enable-libdav1d \
  --disable-indevs --enable-indev=lavfi --disable-outdevs \
  --extra-cflags="-O2 -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET" \
  --extra-ldflags="-mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET" --extra-libs="-liconv"
make -j"$JOBS"
make install

# Hard guard: refuse to continue if anything GPL slipped in.
if grep -E '^#define (CONFIG_GPL|CONFIG_NONFREE|CONFIG_VERSION3) 1' config.h; then
  echo "ERROR: FFmpeg was configured with GPL/nonfree options" >&2; exit 1
fi

# --- mpv: -Dgpl=false, libmpv only ------------------------------------------
# auto_features=disabled turns off everything not listed here. coreaudio = sound,
# gl-cocoa = the OpenGL backend the render API needs, videotoolbox-gl = hardware decoding.
# swift-build: mpv 0.40's macOS code (clipboard etc.) needs its Swift bridge header.
cd "$WORK/mpv"
rm -rf build
meson setup build --prefix="$PREFIX" --libdir=lib --buildtype=release \
  -Dgpl=false -Dlibmpv=true -Dcplayer=false -Dtests=false \
  -Dauto_features=disabled \
  -Dzlib=enabled -Diconv=enabled -Dlibavdevice=enabled \
  -Dcocoa=enabled -Dgl=enabled -Dgl-cocoa=enabled -Dvideotoolbox-gl=enabled \
  -Dcoreaudio=enabled \
  -Dswift-build=enabled -Dmacos-cocoa-cb=disabled -Dmacos-media-player=disabled \
  -Dmacos-touchbar=disabled \
  -Dlua=disabled -Djavascript=disabled \
  --wrap-mode=nodownload
meson compile -C build
meson install -C build
grep -q '#define HAVE_GPL 0' build/config.h || { echo "ERROR: mpv not built with -Dgpl=false" >&2; exit 1; }

# --- collect: libmpv + every dylib it needs, relocatable --------------------
PKG="$OUT/libmpv-lgpl-macos-arm64"
rm -rf "$PKG"; mkdir -p "$PKG/LICENSES" "$PKG/source"
for f in "$PREFIX"/lib/*.dylib; do
  [ -L "$f" ] && continue
  name="$(basename "$(otool -D "$f" | tail -n1)")"
  cp "$f" "$PKG/$name"
done
[ -f "$PKG/libmpv.2.dylib" ] || { echo "libmpv.2.dylib not found" >&2; exit 1; }
for f in "$PKG"/*.dylib; do
  chmod u+w "$f"
  install_name_tool -id "@rpath/$(basename "$f")" "$f"
  otool -L "$f" | tail -n +2 | awk '{print $1}' | while read -r dep; do
    case "$dep" in
      "$PREFIX"/*|@rpath/*) install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$f" ;;
    esac
  done
  otool -l "$f" | grep -q '@loader_path' || install_name_tool -add_rpath @loader_path "$f"
  # install_name_tool breaks the linker's ad-hoc signature; arm64 refuses unsigned code.
  codesign --force --sign - "$f"
done
# Hard guard: an mpv without an audio output plays silently.
strings -a "$PKG/libmpv.2.dylib" | grep -q coreaudio || { echo "ERROR: no coreaudio output" >&2; exit 1; }

# --- licence / attribution / source offer ------------------------------------
cp "$HERE/NOTICE-macos.md" "$PKG/NOTICE.md"
cp "$HERE/REBUILD-macos.md" "$PKG/REBUILD.md"
cp "$WORK/mpv/LICENSE.LGPL" "$PKG/LICENSES/mpv-LGPL-2.1.txt"
cp "$WORK/ffmpeg/COPYING.LGPLv2.1" "$PKG/LICENSES/ffmpeg-LGPL-2.1.txt"
cp "$WORK/libplacebo/LICENSE" "$PKG/LICENSES/libplacebo-LGPL-2.1.txt"
cp "$WORK/dav1d/COPYING" "$PKG/LICENSES/dav1d-BSD-2-Clause.txt"
cp "$WORK/libass/COPYING" "$PKG/LICENSES/libass-ISC.txt"
cp "$WORK/harfbuzz/COPYING" "$PKG/LICENSES/harfbuzz-MIT.txt"
cp "$WORK/fribidi/COPYING" "$PKG/LICENSES/fribidi-LGPL-2.1.txt"
cp "$WORK/freetype/LICENSE.TXT" "$PKG/LICENSES/freetype-LICENSE.txt"
cp "$WORK/freetype/docs/FTL.TXT" "$PKG/LICENSES/freetype-FTL.txt"
for d in mpv ffmpeg libplacebo dav1d libass harfbuzz fribidi freetype; do
  git -C "$WORK/$d" archive --format=tar.gz -o "$PKG/source/$d-$(cat "$WORK/$d.rev" | cut -c1-12).tar.gz" HEAD
done
cp "$HERE/build-macos.sh" "$PKG/source/build-macos.sh"

{
  for d in mpv ffmpeg libplacebo dav1d libass harfbuzz fribidi freetype; do
    printf '%-11s %s\n' "$d" "$(cat "$WORK/$d.rev")"
  done
  echo "refs      mpv $MPV_REF, ffmpeg $FFMPEG_REF, libplacebo $PLACEBO_REF, dav1d $DAV1D_REF, libass $LIBASS_REF, harfbuzz $HARFBUZZ_REF, fribidi $FRIBIDI_REF, freetype $FREETYPE_REF"
  echo "built     $(date -u +%FT%TZ), macOS $(sw_vers -productVersion), min $MACOSX_DEPLOYMENT_TARGET"
  echo "mpv option: -Dgpl=false ; FFmpeg: no --enable-gpl/--enable-nonfree/--enable-version3"
} > "$PKG/BUILDINFO.txt"

(cd "$PKG" && shasum -a 256 *.dylib > SHA256SUMS.txt)
echo "Done: $PKG"
