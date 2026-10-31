#!/usr/bin/env bash
# Builds KONI.app and the .dmg for macOS (Apple Silicon) with the LGPL libmpv package and yt-dlp.
#   client/scripts/build-macos.sh <libmpv-lgpl-macos-arm64.zip | extracted folder>
# The libmpv package is the "libmpv-lgpl-macos-arm64" asset of the libmpv-macos-v* release /
# workflow artifact (docs/LIBMPV.md). Output: client/target/release/bundle/{macos,dmg}/.
# With TAURI_SIGNING_PRIVATE_KEY (+ _PASSWORD) set, the updater archive KONI.app.tar.gz and
# its .sig are produced too (docs/UPDATES.md).
#
# No Apple Developer ID: the app is signed ad hoc ("-"), which Apple Silicon requires to run
# at all. Users confirm the first launch once (README → macOS).
set -euo pipefail

LIBMPV="${1:?usage: build-macos.sh <libmpv package zip or folder>}"
YTDLP_URL="${YTDLP_URL:-https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos}"
CLIENT="$(cd "$(dirname "$0")/.." && pwd)"
VENDOR="$CLIENT/src-tauri/vendor/macos"
rm -rf "$VENDOR"; mkdir -p "$VENDOR/lib" "$VENDOR/licenses"

# libmpv: an artifact zip may contain another zip; accept either, or an extracted folder.
SRC="$LIBMPV"
if [[ "$LIBMPV" == *.zip ]]; then
  SRC="$(mktemp -d)"
  ditto -x -k "$LIBMPV" "$SRC"
  inner="$(find "$SRC" -name 'libmpv-lgpl-macos-arm64.zip' | head -n1)"
  [ -n "$inner" ] && ditto -x -k "$inner" "$SRC"
fi
DYLIB="$(find "$SRC" -name libmpv.2.dylib | head -n1)"
[ -n "$DYLIB" ] || { echo "libmpv.2.dylib not found in $LIBMPV" >&2; exit 1; }
PKG="$(dirname "$DYLIB")"
[ -f "$PKG/NOTICE.md" ] || { echo "NOTICE.md missing: not the LGPL package?" >&2; exit 1; }
# dylibs → Contents/Frameworks; NOTICE.md, LICENSES/, BUILDINFO.txt, REBUILD.md, SOURCE.txt →
# Contents/Resources/libmpv (LGPL).
# Debug symbols are stripped (smaller download; GitHub caps release files kept in git at
# 100 MB). The source tarballs stay in the libmpv release; SOURCE.txt points there (LGPL).
cp "$PKG"/*.dylib "$VENDOR/lib/"
chmod u+w "$VENDOR"/lib/*.dylib
strip -x "$VENDOR"/lib/*.dylib
codesign --force --sign - "$VENDOR"/lib/*.dylib
for f in NOTICE.md REBUILD.md BUILDINFO.txt LICENSES; do
  [ -e "$PKG/$f" ] && cp -R "$PKG/$f" "$VENDOR/licenses/"
done
cat > "$VENDOR/licenses/SOURCE.txt" <<EOT
Exact sources of every library in KONI.app/Contents/Frameworks (mpv, FFmpeg, libplacebo,
libass, HarfBuzz, FriBidi, FreeType, dav1d) and the script that built them:
https://github.com/serkanqltun-hub/koni/releases/tag/${LIBMPV_MAC_RELEASE:-libmpv-macos-v0.40.0-r1}
(libmpv-lgpl-macos-arm64.zip, folder source/). Versions and commits: BUILDINFO.txt.
EOT

curl -fsSL "$YTDLP_URL" -o "$VENDOR/yt-dlp"
chmod 755 "$VENDOR/yt-dlp"

# Tauri config for this build: frameworks list depends on the package contents.
{
  echo '{'
  echo '  "bundle": {'
  echo '    "targets": ["app", "dmg"],'
  echo '    "resources": { "vendor/macos/yt-dlp": "yt-dlp", "vendor/macos/licenses/": "libmpv/" },'
  echo '    "macOS": {'
  echo '      "minimumSystemVersion": "11.0",'
  echo '      "signingIdentity": "-",'
  printf '      "frameworks": ['
  sep=""
  for f in "$VENDOR"/lib/*.dylib; do
    printf '%s"vendor/macos/lib/%s"' "$sep" "$(basename "$f")"
    sep=", "
  done
  echo ']'
  echo '    }'
  echo '  }'
  echo '}'
} > "$CLIENT/src-tauri/vendor/macos.conf.json"
cat "$CLIENT/src-tauri/vendor/macos.conf.json"

configs=(--config src-tauri/vendor/macos.conf.json)
[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && configs+=(--config src-tauri/updater.conf.json)
cd "$CLIENT"
CARGO_PROFILE_RELEASE_STRIP=true npm run tauri -- build "${configs[@]}"

APP="$CLIENT/target/release/bundle/macos/KONI.app"
codesign --verify --deep --strict "$APP"
# Every dylib the app ships must resolve inside the bundle or the OS.
for f in "$APP"/Contents/Frameworks/*.dylib; do
  if otool -L "$f" | tail -n +2 | awk '{print $1}' | grep -v -E '^(@rpath/|/usr/lib/|/System/)'; then
    echo "ERROR: $f links outside the app" >&2; exit 1
  fi
done
ls -la "$CLIENT/target/release/bundle/macos" "$CLIENT/target/release/bundle/dmg"
du -sh "$APP"/Contents/* "$APP"/Contents/Frameworks "$APP"/Contents/Resources/* 2>/dev/null || true
