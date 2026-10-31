# Rebuilding / replacing libmpv

1. Install [MSYS2](https://www.msys2.org), open the **MINGW64** shell.
2. Install build dependencies:
   ```sh
   pacman -S --needed git base-devel mingw-w64-x86_64-toolchain mingw-w64-x86_64-meson \
     mingw-w64-x86_64-ninja mingw-w64-x86_64-pkgconf mingw-w64-x86_64-nasm \
     mingw-w64-x86_64-python mingw-w64-x86_64-libass mingw-w64-x86_64-libplacebo \
     mingw-w64-x86_64-dav1d mingw-w64-x86_64-lcms2 mingw-w64-x86_64-zlib mingw-w64-x86_64-vulkan-headers \
     mingw-w64-x86_64-shaderc mingw-w64-x86_64-spirv-cross
   ```
3. Run `MPV_REF=<tag> FFMPEG_REF=<tag> tools/libmpv/build.sh ./out` (script is in `source/build.sh` of the package).
4. Copy the resulting DLLs next to the KONİ executable (replacing the shipped ones).

Do **not** add `--enable-gpl`, `--enable-nonfree` or `-Dgpl=true`: a GPL build would put
the whole application under the GPL.
