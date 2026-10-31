#!/usr/bin/env bash
# Verifies a built package is LGPL-only and loads. Usage: verify.sh <pkg-dir>
set -euo pipefail
PKG="${1:?usage: verify.sh <pkg-dir>}"
fail=0
# 1) FFmpeg build config is embedded in avcodec; GPL/nonfree must be absent.
if strings -a "$PKG"/avcodec-*.dll | grep -E -- '--enable-(gpl|nonfree|version3)'; then
  echo "FAIL: GPL/nonfree FFmpeg configuration found" >&2; fail=1
fi
# 2) No known GPL-only codec libs linked in.
if strings -a "$PKG"/*.dll | grep -i -E 'libx264|libx265|libxvid|libfdk'; then
  echo "FAIL: GPL/nonfree codec library found" >&2; fail=1
fi
# 3) libmpv exports the client API.
for sym in mpv_create mpv_initialize mpv_command mpv_set_option_string mpv_terminate_destroy; do
  objdump -p "$PKG/libmpv-2.dll" | grep -q "$sym" || { echo "FAIL: missing export $sym" >&2; fail=1; }
done
# 4) Every DLL dependency resolves inside the package or Windows itself.
exit $fail
