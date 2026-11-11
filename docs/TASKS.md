# Nico Karaoke — tasks

Temporary execution plan. Product scope and future ideas live in `docs/roadmap.md`.

Status: Phase 1 in progress.

## Phase 1 — Foundation

Goal: a running walking skeleton with the agreed workspace, a typed frontend build, a local
health endpoint, basic logging, and CI. No media or lyrics behaviour yet.

- [ ] Cargo workspace with `crates/core`, `crates/platform`, and `crates/server`.
- [ ] Shared workspace metadata: edition, version, licence, common dependencies.
- [ ] Core config: host, port, data directory, environment override, bind address.
- [ ] Platform stub with target detection; no MPRIS/SMTC calls yet.
- [ ] Axum server with `GET /api/health` and static file serving.
- [ ] Bind to `127.0.0.1` by default; reject nothing yet beyond the bind address.
- [ ] Logging to stdout with a configurable filter; file logging comes with storage.
- [ ] Vite + strict TypeScript frontend with a minimal i18n setup (`en`, `nl`).
- [ ] Frontend health check against the Rust server in development and production.
- [ ] Unit tests: config parsing, health endpoint, i18n key parity.
- [ ] `README.md` with English run instructions.
- [ ] MIT licence file.
- [ ] GitHub Actions skeleton: Rust fmt/clippy/test and frontend typecheck/test/build.
- [ ] Subagent review of the foundation diff.

Acceptance:

- `cargo test` passes.
- `cargo clippy -- -D warnings` passes.
- `npm test` and `npm run build` pass in `web/`.
- `cargo run -p nico-karaoke-server` serves the built frontend and a healthy API.
- No absolute machine paths or local data in git.

## Phase 2 — Domain and storage

- [ ] LRC parser and writer, including `♪` markers and legacy `[end:]` reading.
- [ ] Timestamp formatting and parsing tests.
- [ ] Track title cleaning and normalisation.
- [ ] Stable record keys and slugged filenames.
- [ ] Versioned `.lrc` + `.json` library store with atomic writes.
- [ ] Manual `.lrc` precedence and metadata-only save rules.
- [ ] Original LRCLIB text preservation.
- [ ] Library listing, search support, delete, and stale-file cleanup.
- [ ] Data-directory and log-file handling.
- [ ] Storage tests with temporary directories.

## Phase 3 — LRCLIB lookup

- [ ] HTTP client behind a trait with a fake implementation for tests.
- [ ] Title cleaning and the `get` → `get` → `search` ladder.
- [ ] Synced-over-plain preference and duration proximity.
- [ ] Candidate deduplication.
- [ ] Found-lyrics cache and one-hour miss cache.
- [ ] Concurrent lookup guard and stale-result guard.
- [ ] Publish/flag left for later.
- [ ] Tests with recorded fixture responses, never the live network.

## Phase 4 — Media providers

- [ ] `MediaProvider` trait, snapshot types, and a fake provider.
- [ ] Linux MPRIS provider.
- [ ] Windows SMTC provider behind conditional compilation.
- [ ] Player selection, fallback selection, and session identity.
- [ ] Play/pause, relative seek, absolute seek, and unsupported-action errors.
- [ ] Position polling and UI interpolation contract.
- [ ] Provider tests with fakes; real-player checks documented separately.

## Phase 5 — HTTP API

- [ ] State endpoint for current player and lyrics.
- [ ] Player selection endpoint.
- [ ] Playback control endpoint.
- [ ] Library list, item, save, create, delete, and original endpoints.
- [ ] Lyrics lookup endpoint for the current and selected track.
- [ ] Offset endpoint.
- [ ] Stable error codes for the frontend.
- [ ] Host/Origin validation, JSON content type, body limits, no-store.
- [ ] API tests with fake services.

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

- [ ] Studio route and loading by key or current track.
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
