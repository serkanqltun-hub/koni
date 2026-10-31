# Rebuilding / replacing libmpv (macOS)

1. Install the Xcode command line tools (`xcode-select --install`) and Homebrew build tools:
   `brew install meson ninja nasm pkg-config`
2. Run `MPV_REF=<tag> FFMPEG_REF=<tag> source/build-macos.sh ./out`.
3. Replace the dylibs in `KONI.app/Contents/Frameworks` with the ones from
   `out/libmpv-lgpl-macos-arm64`, then re-sign the app ad hoc:
   `codesign --force --deep --sign - KONI.app`

Do **not** add `--enable-gpl`, `--enable-nonfree` or `-Dgpl=true`: a GPL build would put
the whole application under the GPL.
