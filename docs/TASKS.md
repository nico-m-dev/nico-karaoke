# Nico Karaoke — tasks

Temporary execution plan. Product scope and future ideas live in `docs/roadmap.md`.

Status: Phase 5 complete. Phase 6 is next.

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

- [ ] Idle screen with library entry point and language switch.
- [ ] Track title, artist, player picker, and playback controls.
- [ ] Line rendering with centre line, green fill, upcoming/past styles, and `♪` breaks.
- [ ] Local position interpolation and smooth scrolling.
- [ ] Plain-text fallback for unsynchronised lyrics.
- [ ] Chrome auto-hide, fullscreen, and keyboard shortcuts.
- [ ] Offset application.
- [ ] Accessibility and reduced-motion pass.
- [ ] Frontend tests for fill and active-line calculations.

## Phase 7 — Library UI

- [ ] Search by title, artist, and album.
- [ ] Item cards with source, sync state, line count, date, and original badge.
- [ ] Edit, delete with confirmation, refresh, and external search links.
- [ ] Empty, loading, error, and no-results states.

## Phase 8 — Studio UI

- [ ] Studio route and loading by key or current track. The Studio is a server-rendered route
  (`/editor`), not a client-side catch-all; unknown paths stay 404.
- [ ] Header, dirty state, save, fetch, restore original, raw LRC.
- [ ] Timeline with rulers, markers, playhead, zoom, fit, click-to-seek, drag-to-retime.
- [ ] Line list with inline time/text editing, sorting, add line, add music, delete.
- [ ] Inspector with nudges, take-current-time, preview, music toggle, delete.
- [ ] Tap mode with countdown, correction, and backspace.
- [ ] Undo stack and keyboard shortcuts.
- [ ] Legacy `[end:]` migration when opening an item.
- [ ] Unsaved-changes guard.
- [ ] Studio tests and manual acceptance matrix.

## Phase 9 — Hardening

- [ ] Error states for no player, no position, no seek, no lyrics, and network failure.
- [ ] Logging and log-file rotation policy.
- [ ] Security review of static serving, Host/Origin, and body limits.
- [ ] Review dev-tooling dependency advisories; runtime dependencies must stay clean.
- [ ] Accessibility review.
- [ ] Long-song and long-line performance check.
- [ ] Linux and Windows manual player matrix.

## Phase 10 — Packaging

- [ ] Embedded frontend assets in the release binary.
- [ ] Linux and Windows release builds in CI.
- [ ] Icon, version metadata, and no console window on Windows.
- [ ] Tauri shell using the same core, API, and UI.
- [ ] Git tags, changelog, and GitHub Release assets.
- [ ] Optional update notification, later.
