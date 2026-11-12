# Nico Karaoke — tasks

Temporary execution plan. Product scope and future ideas live in `docs/roadmap.md`.

Status: Phase 9 complete on Linux. The Windows SMTC run and packaging wait for the owner.

## Phase 1 — Foundation

Goal: a running walking skeleton with the agreed workspace, a typed frontend build, a local
health endpoint, basic logging, and CI. No media or lyrics behaviour yet.

- [x] Cargo workspace with `crates/core`, `crates/platform`, and `crates/server`.
- [x] Shared workspace metadata: edition, version, licence, common dependencies.
- [x] Core config: host, port, data directory, environment override, bind address.
- [x] Platform stub with target detection; no MPRIS/SMTC calls yet.
- [x] Axum server with `GET /api/health` and static file serving.
- [x] Bind to `127.0.0.1` by default; reject nothing yet beyond the bind address.
- [x] Logging to stdout with a configurable filter; file logging comes with storage.
- [x] Vite + strict TypeScript frontend with a minimal i18n setup (`en`, `nl`).
- [x] Frontend health check against the Rust server in development and production.
- [x] Unit tests: config parsing, health endpoint, i18n key parity.
- [x] `README.md` with English run instructions.
- [x] MIT licence file.
- [x] GitHub Actions skeleton: Rust fmt/clippy/test and frontend typecheck/lint/format/test/build.
- [x] Subagent review of the foundation diff and fixes for its findings.

Acceptance:

- [x] `cargo test --workspace --locked` passes.
- [x] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.
- [x] `npm test`, `npm run lint`, `npm run format:check`, and `npm run build` pass in `web/`.
- [x] `cargo run -p nico-karaoke-server` serves the built frontend and a healthy API.
- [x] No absolute machine paths or local data in git.
- [x] `--port 0`, `--help`, `--version`, bare `/api`, and missing static paths behave correctly.

## Phase 2 — Domain and storage

- [x] LRC parser and writer, including `♪` markers and legacy `[end:]` reading.
- [x] Timestamp formatting and parsing tests.
- [x] Track title cleaning and normalisation.
- [x] Stable record keys and slugged filenames.
- [x] Versioned `.lrc` + `.json` library store with atomic writes.
- [x] Manual `.lrc` precedence and metadata-only save rules.
- [x] Original LRCLIB text preservation.
- [x] Library listing, search support, delete, and stale-file cleanup.
- [x] Data-directory and log-file handling.
- [x] Storage tests with temporary directories.

Acceptance:

- [x] `cargo test --workspace --locked` passes.
- [x] A metadata-only save keeps a newer hand-edited `.lrc`.
- [x] An explicit new text save overrides a hand-edited `.lrc`.
- [x] Reopening the library preserves text, source, offset, original, and sync state.
- [x] The server creates `nico-karaoke.log` in the data directory.
- [x] No temporary files, build output, or local data are committed.

## Phase 3 — LRCLIB lookup

- [x] HTTP client behind a trait with a fake implementation for tests.
- [x] Title cleaning and the `get` → `get` → `search` ladder.
- [x] Synced-over-plain preference and duration proximity.
- [x] Candidate deduplication.
- [x] Found lyrics returned as library records; one-hour miss cache in the service.
- [x] Concurrent lookup guard. The stale-result guard belongs to the watcher in Phase 4.
- [x] Publish/flag explicitly out of scope for this phase.
- [x] Tests with fixture responses, never the live network.

## Phase 4 — Media providers

- [x] `MediaProvider` trait, snapshot types, and a fake provider.
- [x] Linux MPRIS provider via zbus; helper tests and a manual probe example.
- [x] Windows SMTC provider behind conditional compilation; cross-checked with a Windows target.
- [x] Player selection, fallback selection, and session identity.
- [x] Play/pause, relative seek, absolute seek, and unsupported-action errors.
- [x] Position polling and UI interpolation contract.
- [x] Track generation primitive for the stale-result guard; applying it lands with the watcher.
- [x] Provider tests with fakes; real-player checks use `examples/media_probe.rs`.

## Phase 5 — HTTP API

- [x] State endpoint for current player and lyrics.
- [x] Player selection endpoint.
- [x] Playback control endpoint.
- [x] Library list, item, save, create, delete, and original endpoints.
- [x] Call synchronous library I/O through `spawn_blocking` so Axum worker threads stay free.
- [x] Run lookups in a background task and apply results only when the media generation is unchanged. A player switch can change the generation without a `track_changed` update, so key the lookup on generation.
- [x] Add a short failure cooldown so an outage does not cause a request on every poll.
- [x] Lyrics lookup endpoint for the current and selected track.
- [x] Offset endpoint.
- [x] Stable error codes for the frontend.
- [x] Host/Origin validation, JSON content type, body limits, no-store.
- [x] API tests with fake media and lyrics services.

Acceptance:

- [x] `GET /api/now` returns track, players, and lyrics state.
- [x] Background lookup saves found lyrics and survives a track change without applying stale results.
- [x] Library create/read/save/delete/original and offset round-trip through the API.
- [x] Foreign Host/Origin, wrong content type, and invalid JSON return stable JSON error codes.
- [x] The real server starts with no media player and serves the state and library endpoints.

## Phase 6 — Player UI

- [x] Idle screen with library entry point and language switch.
- [x] Track title, artist, player picker, and playback controls.
- [x] Line rendering with centre line, green fill, upcoming/past styles, and `♪` breaks.
- [x] Local position interpolation and smooth scrolling.
- [x] Plain-text fallback for unsynchronised lyrics.
- [x] Chrome auto-hide, fullscreen, and keyboard shortcuts.
- [x] Offset application.
- [x] Accessibility and reduced-motion pass.
- [x] Frontend tests for fill and active-line calculations.

Acceptance:

- [x] The idle screen and a playing track were rendered in headless Chromium against a fake MPRIS player.
- [x] Synced lines scroll to the centre line, fill green, and stop filling at `♪` markers.
- [x] Plain lyrics render as a scrollable list without the centre line.
- [x] No-player, loading, missing, and error states have readable messages.
- [x] NL/EN switch, player picker, play/pause, library entry point, fullscreen, and keyboard shortcuts work.
- [x] `npm test`, `npm run typecheck`, `npm run lint`, `npm run format:check`, and `npm run build` pass.

## Phase 7 — Library UI

- [x] Search by title, artist, and album.
- [x] Item cards with source, sync state, line count, date, and original badge.
- [x] Detail view with raw-text editing and save.
- [x] Restore the saved original with confirmation.
- [x] Delete with confirmation.
- [x] Refresh and current-track badge.
- [x] External YouTube and Spotify search links.
- [x] Empty, loading, error, and no-results states.

Acceptance:

- [x] The library opens from the idle and playing views.
- [x] Search filters the visible items without a page reload.
- [x] A detail view loads raw text, saves changes through `PUT /api/library/{key}`, and restores originals.
- [x] Delete removes the item and refreshes the list.
- [x] Focus stays inside the dialog; closing restores the opener.
- [x] Live Chromium check with two items: search, edit/save, restore original, delete, and external links.
- [x] `npm test`, `npm run typecheck`, `npm run lint`, `npm run format:check`, and `npm run build` pass.

## Phase 8 — Studio UI

- [x] Studio route and loading by key or current track. The Studio is a real server route
  (`/editor`) with its own Vite entry; unknown paths stay 404.
- [x] Header, dirty state, save, fetch, restore original, raw LRC.
- [x] Timeline with rulers, markers, playhead, zoom, fit, click-to-seek, drag-to-retime.
- [x] Line list with inline time/text editing, sorting, add line, add music, delete.
- [x] Inspector with nudges, take-current-time, preview, music toggle, delete.
- [x] Tap mode with countdown, correction, and backspace.
- [x] Undo stack and keyboard shortcuts.
- [x] Legacy `[end:]` migration when opening an item.
- [x] Unsaved-changes guard.
- [x] Server-side raw-LRC parsing endpoint so the editor and player share one parser.
- [x] Studio tests and manual acceptance matrix.

Acceptance:

- [x] `/editor` and `/editor?key=…` load in the real server and in Vite development; unknown
  routes still return 404.
- [x] The Studio opens from a library detail and directly for the current track.
- [x] Timeline markers select and seek, dragging retimes with undo, the ruler seeks, labels
  hide when they do not fit, and zoom/fit work.
- [x] Inline text and time edits mark the item dirty, re-sort, and persist through an explicit
  save.
- [x] Add line, add music, delete, music toggle, nudges, take-current-time, and undo work.
- [x] Preview pauses, seeks 1.5 s before the line, plays, and pauses 1.5 s after it.
- [x] Tap mode counts down, starts at zero, applies the configured correction, accepts
  Backspace, and returns to the editor.
- [x] Legacy `[end:]` tags become `♪` markers with a visible notice until saved.
- [x] Raw LRC apply, restore original, fetch, and the unsaved-changes guard behave correctly.
- [x] NL/EN labels update without losing the document.
- [x] `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, the Windows platform check,
  and `npm` typecheck/lint/format/test/build pass.
- [x] Live Chromium checks against a stateful fake MPRIS player cover the flows above.

## Phase 9 — Hardening

- [x] Error states for no player, no position, no seek, no lyrics, and network failure.
- [x] Logging and log-file rotation policy.
- [x] Security review of static serving, Host/Origin, and body limits.
- [x] Review dev-tooling dependency advisories; runtime dependencies must stay clean.
- [x] Accessibility review.
- [x] Long-song and long-line performance check.
- [x] Linux MPRIS matrix with a real player.
- [ ] Windows SMTC matrix on a real Windows machine. The crate compiles; the runtime path is
  untested and this stays open for packaging.

Acceptance:

- [x] A player without a position shows `—` instead of `0:00`; the player keeps working in
  plain-text mode.
- [x] A server outage shows a persistent "Server offline" badge and a one-time toast, and the
  badge clears when the server returns. The Studio reports the outage once without dropping
  edits.
- [x] No player, no lyrics, missing lyrics, unavailable lyrics, unsupported seek, and network
  errors have readable states or messages.
- [x] The log rotates at 5 MiB to `.1`, keeps three backups, and starts a fresh file; unit
  tests cover the shift and the backup cap.
- [x] Security headers and a self-only CSP are present on API responses, static files, and
  rejected requests. Chromium reported no CSP violations while loading the player, library,
  and Studio.
- [x] Encoded and plain path traversal attempts are rejected; oversized bodies, wrong
  content types, invalid JSON, and foreign `Host`/`Origin` values return stable error codes.
- [x] `npm audit` reports zero advisories after pinning the fixed `minimatch`; runtime
  dependencies stay clean.
- [x] Metadata contrast, large-text contrast for inactive lyric lines, heading levels, focus
  visibility, labels, live regions, dialog focus traps, inert tap mode, and reduced motion
  were reviewed.
- [x] A three-hour, 2000-line track renders in the player and Studio without horizontal
  overflow; a 1505-character line wraps. Measurements are recorded in `docs/testing.md`.
- [x] A real `mpv` 0.41 session with `mpv-mpris` was discovered, and play, pause, absolute
  seek, position, and duration worked. Two fake MPRIS players covered selection and
  fallback.
- [x] `cargo fmt`, clippy, the full Rust suite, the Windows platform check, and web
  typecheck/lint/format/test/build pass.

## Follow-up — owner test round (2026-11-12)

- [x] Restore the lime-green accent from the Python app.
- [x] Put the idle language switch below the Library button and remove the em dash from the
  idle lead.
- [x] Simplify the library detail: read-only preview plus Studio, YouTube, Spotify, and
  Delete. Text editing and restoring moved fully to the Studio.
- [x] Match a library record when the active provider omits the artist or adds video/channel
  decorations, using duration to disambiguate covers.
- [x] Keep the selected timeline marker in place while zooming.
- [x] Treat timed lines without text as `♪` music markers in the Studio, matching the player.
- [x] Replace the misleading per-line play button with an explicit "go to line" action.
- [x] Move tap mode out of the inspector into the transport bar and show previous and next
  lines around the line being tapped.
- [x] Stop animation frames while playback is paused and halve Studio polling.
- [x] Add tests for metadata matching and blank-line music markers; re-run the live checks.

## Phase 10a — Tauri shell (local)

Goal: the app in its own native window, loading the built frontend through the same local
server. No packaging, bundling, Updater, GitHub, or Actions work in this step.

- [x] Make the Axum startup reusable so the browser binary and the desktop shell share the
  same config, library, watcher, and server setup.
- [x] Add `src-tauri/` with Tauri 2, a build script, and a minimal window configuration.
- [x] Start the local server automatically inside Tauri and open the window at its bound
  `127.0.0.1` URL.
- [x] Default to the built `web/dist` (production-like). Allow an explicit dev override to
  the Vite URL for HMR.
- [x] Reuse the data directory and log file from the browser build.
- [x] Keep `cargo run -p nico-karaoke-server` and the browser fallback working unchanged.
- [x] Build and launch locally, verify the window, server startup, close behaviour, and a
  clean shutdown without orphaning the server.
- [x] Leave icons, `.deb`/AppImage, embedded assets, Windows packaging, and the owner's
  release checkbox for Phase 10.

Acceptance:

- [x] `cargo run -p nico-karaoke-server` still serves the browser UI and `/editor`.
- [x] `cargo run -p nico-karaoke-desktop` starts the server, opens a native window titled
  `Nico Karaoke`, and loads the built UI at `127.0.0.1:8770` (or the next free port).
- [x] The desktop window uses the same data directory and log file.
- [x] Stopping the process closes the server with it; no orphaned listener remains.
- [x] The desktop icon is documented as a local placeholder.
- [x] The webview URL override only accepts local `http` URLs; remote content is refused.

## Phase 10 — Packaging

Before packaging starts, the owner runs the app in development and confirms it is ready:

- [ ] Owner acceptance run in dev: use the app end to end locally and confirm packaging may
  start. This box belongs to the owner; agents wait until it is checked.

Then:

- [ ] Run the SMTC acceptance matrix on a real Windows machine before a Windows release.
- [ ] Embedded frontend assets in the release binary.
- [ ] Linux and Windows release builds in CI.
- [ ] Icon, version metadata, and no console window on Windows.
- [ ] Tauri shell using the same core, API, and UI.
- [ ] Git tags, changelog, and GitHub Release assets.
- [ ] Optional update notification, later.
