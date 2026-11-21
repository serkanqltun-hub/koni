# CLAUDE.md

Context for AI coding assistants (Claude Code, Cursor, Copilot, …) and the people driving them.
Read this before changing code. Human setup steps are in [CONTRIBUTING.md](CONTRIBUTING.md).

## What KONİ is

A desktop app for watching and listening together. Each member plays media **on their own
machine**; a small Go server only shares the room clock, room state and accounts. No media
ever goes through the server.

| Part | Stack | Where |
|:--|:--|:--|
| Desktop app | Tauri 2 (Rust) + Svelte 5 + TypeScript, Vite | `client/` |
| Video/audio | libmpv, loaded at runtime (never linked) | `client/crates/koni-player` |
| Video surface | `VideoSurface` trait: Win32 child window / macOS OpenGL view | `client/crates/koni-surface` |
| Sync engine | drift control, buffering rules, pure logic + simulator | `client/crates/koni-sync` |
| URL resolving | URL classification + yt-dlp | `client/resolver` |
| Sync server | Go, WebSocket, SQLite (`modernc.org/sqlite`, no cgo) | `server/` |
| Deploy | systemd, nginx, `install.sh` | `deploy/` |
| libmpv builds | LGPL build scripts for Windows and macOS | `tools/libmpv`, `.github/workflows/libmpv-*.yml` |

Key files:
- `client/src-tauri/src/lib.rs`: app setup, player and surface init.
- `client/src-tauri/src/commands.rs`: IPC commands. `sync.rs`: room session. `media.rs`: opening media.
- `client/src/App.svelte` and `client/src/lib/*.svelte`: UI. `client/src/lib/player.ts`: IPC wrappers.
- `server/hub/`: rooms (one goroutine actor per room), accounts, social, admin. `server/store/`: SQLite.
- `docs/PROTOCOL.md`: the WebSocket protocol. `docs/adr/`: decisions. `docs/design/`: design notes.

Many internal docs and code comments are in Turkish. That's fine; new repo-facing text
(README, CONTRIBUTING, PR descriptions) should be English.

## Boundaries: do not break these

1. **Cockpit layout, no overlay on video** ([ADR 0001](docs/adr/0001-kokpit-modeli.md)).
   Video is a native surface (left/top); the web UI lives only in the right column and bottom
   bar. Never draw HTML over the video area or make the webview transparent over it.
2. **Platform code stays behind `VideoSurface`** ([ADR 0003](docs/adr/0003-videosurface.md)).
   Windows and macOS specifics go in `koni-surface` (`win32.rs`, `macos.rs`) or behind
   `#[cfg(target_os = …)]`. Svelte, IPC and the protocol must not fork per platform.
   On macOS mpv can't embed via `wid`, so it renders with `vo=libmpv` + the render API.
3. **Never seek a member backwards to fix drift** ([ADR 0004](docs/adr/0004-geri-seek-yok.md)).
   Small drift is fixed with 0.96–1.04× speed and pitch correction; see `docs/design/sync.md`.
   Changes to `koni-sync` need tests (`cargo test -p koni-sync`).
4. **libmpv is LGPL and dynamic** ([ADR 0006](docs/adr/0006-libmpv-lgpl.md)).
   Load it with `libloading`; never link it statically, never pin its hash, never switch the
   shipped build to a GPL/nonfree mpv or FFmpeg. Dev-only DLLs may be GPL; shipped ones may not.
5. **The protocol only grows** (`docs/PROTOCOL.md`). Add fields/messages, don't rename or
   remove them; old clients ignore unknown fields. A protocol change means: bump
   `ProtocolVersion` in `server/hub/messages.go`, add a changelog line at the top of
   `docs/PROTOCOL.md`, and add server tests in `server/hub/*_test.go`.
6. **No built-in server.** The app has no default server address; it comes from an invite link
   (`koni://invite/CODE?s=…`) or the user types it. Don't hard-code any server address,
   domain or IP in code, docs or tests (use `koni.example.com` / `localhost`).
7. **Privacy by default** ([ADR 0005](docs/adr/0005-sifir-pii.md)). Invite + username +
   argon2id password only; no email, no third-party login, no telemetry. Stats and lists are
   visible to mutual friends only. Network options that route traffic through the server are
   off by default; keep them that way.
8. **Content-neutral.** KONİ is a player. Don't add bundled media sources, catalogues of
   streams, or features whose purpose is getting around a site's access rules. Metadata
   (titles, posters, ratings) comes from public metadata services.
9. **Tauri permissions.** A new IPC command must be added to the `COMMANDS` list in
   `client/src-tauri/build.rs` **and** to `client/src-tauri/capabilities/default.json`
   (`allow-<command-name>`), or it fails with "not allowed by ACL". Never give the
   in-app browser windows (`capabilities/sniffer.json`, Discover) more commands than they have.
10. **Two languages.** UI strings are written in Turkish and translated in
    `client/src/lib/i18n/*.ts` (key = Turkish text, value = English). Rust strings use
    `tr!("Türkçe", "English")`. Add both for every user-visible string.
11. **Don't bump versions or touch release plumbing.** Versions (`tauri.conf.json`,
    `client/Cargo.toml`), `docs/releases/`, `release.yml` and the updater keys are the
    maintainer's job. Releases are cut in batches.
12. **Keep it small.** The maintainer prefers simple, concrete changes over new abstractions,
    frameworks or dependencies. One feature or fix per PR.

## Checks to run before a PR

```sh
# client (from client/)
npm ci && npm run build && npm run check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings   # Windows/macOS only (src-tauri)
cargo test --workspace                                  # Linux: -p koni-player -p koni-surface -p koni-sync

# server (from server/)
go vet ./... && go test -race ./...
```

CI (`.github/workflows/client.yml`) builds and tests on Windows and macOS for every PR that
touches `client/`. The macOS `.app`/`.dmg` is built only when the PR has the `macos` label.

## Platform notes

- **Windows 10/11** is the main, tested platform.
- **macOS (Apple Silicon)** is built in CI but the maintainer has no Mac, so it is barely
  tested on real hardware. The app is ad hoc signed, not notarized, with hardened runtime off
  (needed so the bundled ad hoc signed dylibs load). Don't add signing/notarization steps.
- **Linux**: the server and the pure crates build; the desktop app is not supported yet.
