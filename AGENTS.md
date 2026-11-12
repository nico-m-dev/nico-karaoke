# Nico Karaoke — agent guide

## What this project is

Nico Karaoke shows synchronised lyrics for whatever the user is playing in another
application. It reads track metadata and playback position from MPRIS (Linux) or SMTC
(Windows), fetches lyrics from LRCLIB, and draws a line-by-line karaoke view in a local web
UI.

The app does **not** record audio, download tracks, play video, or require an account.

## Status

The rewrite is implemented: Rust backend, strict TypeScript player, library, and Studio, a
browser fallback, and a Tauri 2 desktop shell that starts the same local server. Remaining
work is release acceptance, finetuning, a real Windows SMTC run, and packaging.

## Language and writing rules

- **English is the source language** for code, identifiers, comments, tests, commit
  messages, documentation, and source UI strings.
- Dutch is a **translation**, never the source. Keep `nl` complete when adding user-facing
  text.
- Write commit messages in English.
- Do not put absolute machine paths in documentation, code comments, examples, or tests.
  Use repository-relative paths, environment variables, or generic platform locations.
- Use ISO 8601 dates (`2026-11-11`) in documentation and changelogs.
- Prefer short, factual documentation over marketing copy.

## Stack

- **Backend:** Rust, stable toolchain, Tokio, Axum, Serde, Reqwest.
- **Media:** MPRIS through `zbus`/`mpris` on Linux; SMTC through the `windows` crate on
  Windows. Platform code stays behind a common trait.
- **Frontend:** TypeScript with strict compiler settings and Vite. Keep the player
  framework-free unless a framework is explicitly agreed; the lyrics view needs precise,
  cheap animation.
- **Desktop shell:** Tauri 2 in `src-tauri`, sharing the server runtime in `crates/server`.
  `npm run app` builds the web UI and starts the shell; `npm run app:hmr` adds Vite hot
  reload. The core, API, and web UI must keep working without Tauri so the browser fallback
  stays usable.
- **Shared types:** API shapes are currently mirrored in `web/src/api.ts` from the Rust DTOs.
  Keep them in sync; generating one shared contract remains an open improvement.

Do not add a dependency, framework, or build tool without agreeing on it first.

## Architecture boundaries

- Domain logic (lyrics parsing, title cleaning, library rules, playback state) must not
  depend on HTTP, a window toolkit, or a specific desktop shell.
- Media access sits behind a provider trait so tests can use a fake player.
- LRCLIB access sits behind a client trait so tests never call the network.
- HTTP handlers validate input and call domain services; they do not touch files directly.
- Library writes are atomic and serialised. A failed write must not leave a half-written
  record.
- The UI polls state but interpolates playback position locally; do not add per-frame server
  requests. Animation loops stop while playback is paused.
- Centre the lyric track with native viewport scrolling, not a CSS transform. WebKitGTK at
  fractional display scaling visibly jitters transformed text; see `docs/roadmap.md`.

## Product rules

- Localhost only by default. Validate `Host` and `Origin` on every request. The Tauri
  window loads only the local server; a dev override is restricted to local `http` URLs.
- Never add telemetry, analytics, remote logging, or automatic uploads.
- Never record or capture audio.
- Lyrics are fetched, not downloaded as media. Playback stays in the user's own player.
- Destructive actions need explicit confirmation in the UI.
- Public sharing or publishing of lyrics is always opt-in and confirmed.
- The player view stays deliberately quiet: playback, library, Studio, fullscreen. Editing
  belongs in the Studio.

## Data format

- Use a clean, versioned, file-based library: one human-readable `.lrc` file plus one
  `.json` metadata file per song. There is no requirement to read the old Python library
  or its legacy cache.
- A manual `.lrc` edit wins over the JSON copy. Metadata-only updates must not overwrite
  newer manual text.
- Keep the saved original LRCLIB text separate from the working text.
- Version the JSON schema from the first release. A future schema change needs a documented
  version bump, a backup path, and tests.
- Write files atomically and serialise library writes.
- Use the platform data directory; allow an environment-variable override for development
  and tests.

## Localisation

- English is the default UI language. Dutch is available and persisted when selected.
- Every user-facing string goes through the translation catalog. No hardcoded visible text
  in components or handlers.
- Backend responses expose stable machine-readable error codes, not translated prose. The
  frontend translates them.
- Keep `en` and `nl` key sets identical. A test must fail when they drift.
- Adding a language must require only a new catalog plus registration; document the steps.

## Coding standards

- Run `cargo fmt` and `cargo clippy -- -D warnings` before finishing Rust work.
- Run `tsc --noEmit`, the formatter, and the linter before finishing TypeScript work.
- Tests must cover LRC parsing, title cleaning, the LRCLIB lookup ladder, library
  persistence and schema versioning, API validation, and translation key parity.
- Unit tests must not depend on a real player, D-Bus, SMTC, or the network.
- Avoid `unwrap()`/`expect()` on runtime input, filesystem, network, or platform calls.
  Return structured errors or log and degrade gracefully.
- Never log secrets, tokens, full user paths, or complete lyric bodies at normal log level.
- Keep functions and modules small enough to test in isolation.

## Git and repository hygiene

- Conventional Commits in English (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`,
  `chore:`).
- Commit small, coherent changes. Do not commit build output, local data, logs, caches, or
  editor state.
- `Cargo.lock` and the Node lockfile are committed because this is an application.
- Do not commit generated media or icons unless the source and licence are documented.
- Maintain `.gitignore` as the stack grows. In particular: `target/`, `node_modules/`,
  frontend build output, local data directories, logs, `.env` files, and OS/editor files.

## Workflow

- For a new feature, multi-file refactor, data migration, release, or anything that goes
  live, keep a temporary checklist in `docs/TASKS.md`, organised by the current workstream
  (acceptance, verification, packaging, or an active follow-up).
- `docs/roadmap.md` owns product scope and future ideas. `docs/TASKS.md` owns the current
  execution plan. Move items between them deliberately.
- Before saying something works, run it. Static checks alone are not evidence of runtime
  behaviour.
- Have a reviewer or subagent read the diff and run the tests before calling large work done.
- `docs/testing.md` records automated evidence, the manual player matrix, accessibility
  notes, and performance measurements.
- When work finishes, remove completed checklist items and leave only permanent decisions in
  `README.md`, `docs/roadmap.md`, or `AGENTS.md`.

## Definition of done

- Behaviour is implemented and exercised locally.
- Tests and linters pass.
- English source text and Dutch translation are both complete.
- No absolute paths, no secrets, no local data in the repository.
- Relevant docs are updated, including `docs/roadmap.md`, `docs/TASKS.md`, or
  `docs/testing.md` when scope or evidence changed.
