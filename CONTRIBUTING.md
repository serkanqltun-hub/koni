# Contributing to KONİ

Thanks for helping! KONİ is a one-person project, so small, focused PRs are the easiest to
review. Using an AI assistant is fine: point it at [CLAUDE.md](CLAUDE.md) first, it lists the
rules that keep the architecture intact.

- **Bugs and ideas**: open an issue with one of the templates.
- **Mac users**: the maintainer has no Mac. Testing a release and filing *macOS feedback* is
  one of the most useful things you can do.
- **PRs**: one fix or feature per PR, run the checks below, and describe how you tested it.
  Don't bump the version; releases are cut in batches.

## Repository layout

| Folder | What | You need |
|:--|:--|:--|
| `client/` | Desktop app: Tauri 2 (Rust) + Svelte 5 | Node 22, Rust stable, libmpv |
| `server/` | Sync server: Go + SQLite | Go (version in `server/go.mod`) |
| `docs/` | Protocol, design notes, decisions (partly in Turkish) | |

## 1. Run the server locally (any OS, 1 minute)

```sh
cd server
go test ./...
go run ./cmd/koni-server -addr 127.0.0.1:8080 -db koni.db
# in another terminal: print a one-time invite code for the first account
go run ./cmd/koni-server invite -db koni.db
```

In the app, enter `localhost:8080` as the server, then sign up with that code. The first
account becomes admin and can create more invites from inside the app.

## 2. Run the desktop app

**You don't need libmpv for UI work.** Without it the app still opens and only video/audio
playback is missing (you'll see "couldn't load libmpv"). Add libmpv when you work on playback.

The app is a single instance per machine, so testing sync between two members needs two
computers (or a friend).

### Windows 10/11

1. Install [Node 22](https://nodejs.org), [Rust](https://rustup.rs) (MSVC toolchain, with
   the "Desktop development with C++" build tools it asks for) and the
   [GitHub CLI](https://cli.github.com/). WebView2 is already on Windows 10/11.
2. Install dependencies:
   ```powershell
   cd client
   npm ci
   New-Item -ItemType Directory -Force target\debug | Out-Null
   ```
3. Put libmpv next to the debug exe. The project's own LGPL build is a release in this repo:
   ```powershell
   gh release download libmpv-v0.40.0-r2 -R serkanqltun-hub/koni -p libmpv-lgpl-win64.zip -D $env:TEMP\libmpv
   Expand-Archive -Force $env:TEMP\libmpv\libmpv-lgpl-win64.zip $env:TEMP\libmpv
   Get-ChildItem $env:TEMP\libmpv -Recurse -Filter *.dll | Copy-Item -Destination target\debug
   ```
   (Already have KONİ installed? `scripts/dev.ps1` copies the DLLs from your install instead.)
4. Optional, for links and Discover: put [`yt-dlp.exe`](https://github.com/yt-dlp/yt-dlp/releases)
   in `target\debug` or on your `PATH`.
5. Run it: `npx tauri dev`. UI changes reload instantly; Rust changes rebuild.

### macOS (Apple Silicon)

1. Install Xcode Command Line Tools (`xcode-select --install`), Node 22, Rust and the GitHub
   CLI (e.g. `brew install node@22 rustup gh && rustup-init`).
2. Install dependencies:
   ```sh
   cd client
   npm ci
   ```
3. Put the libmpv dylibs where the debug app looks for them (`client/target/Frameworks`,
   i.e. `../Frameworks` next to `target/debug/koni`):
   ```sh
   gh release download libmpv-macos-v0.40.0-r1 -R serkanqltun-hub/koni -p libmpv-lgpl-macos-arm64.zip -D /tmp/libmpv
   ditto -x -k /tmp/libmpv/libmpv-lgpl-macos-arm64.zip /tmp/libmpv
   mkdir -p target/Frameworks
   find /tmp/libmpv -name '*.dylib' -exec cp {} target/Frameworks/ \;
   ```
   If you downloaded the zip with a browser instead, clear the quarantine flag:
   `xattr -dr com.apple.quarantine target/Frameworks`.
4. Optional: `brew install yt-dlp` for links and Discover.
5. Run it: `npx tauri dev`.
6. To build the real `KONI.app` and `.dmg`: `./scripts/build-macos.sh /tmp/libmpv/libmpv-lgpl-macos-arm64.zip`.

Quick check that libmpv loads, without the app:
```sh
cargo run -p koni-player --example smoke -- --lib target/Frameworks/libmpv.2.dylib --render-sw   # macOS
cargo run -p koni-player --example smoke -- --lib target\debug\libmpv-2.dll                     # Windows
```

### Linux

The desktop app isn't supported yet. You can still work on the server and on the pure Rust
crates: `cargo test -p koni-player -p koni-surface -p koni-sync`.

## 3. Let CI do the heavy lifting

You don't have to build everything locally. Every PR that touches `client/` runs
`.github/workflows/client.yml`:

- **Linux**: `cargo fmt`, tests and clippy for the pure crates.
- **Windows**: frontend build, clippy, tests, libmpv smoke test.
- **macOS**: the same, plus a frame drawn through the libmpv render API.
  Add the `macos` label to the PR (the maintainer can do it) to also build `KONI.app` + `.dmg`
  and try launching it.

Building libmpv itself is only needed when changing mpv/FFmpeg versions; see
[docs/LIBMPV.md](docs/LIBMPV.md). Shipped builds must stay LGPL.

## 4. Before you open a PR

```sh
# client/
npm run build && npm run check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# server/
go vet ./... && go test -race ./...
```

- New UI text: write it in Turkish in the component and add the English line in
  `client/src/lib/i18n/` (see [CLAUDE.md](CLAUDE.md)).
- New IPC command: add it to `client/src-tauri/build.rs` and `capabilities/default.json`.
- Protocol change: additions only, bump `ProtocolVersion`, note it in `docs/PROTOCOL.md`.
- Don't commit server addresses, keys, logs or `.db` files.

By contributing you agree your work is released under the [MIT License](LICENSE).
