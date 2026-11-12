# Nico Karaoke — roadmap

Status: the rewrite is delivered; release readiness and product follow-ups remain.
Updated: 2026-11-12.

This file owns product direction and future work. Verified behaviour and manual evidence live
in `docs/testing.md`. When an item is delivered, remove it here or mark the lasting decision;
do not keep a history of finished phases.

## What makes this karaoke app

Nico Karaoke is a local lyrics follower, not a media player:

- It follows the track already playing in another application.
- It shows line-synchronised lyrics with a green progress line in the centre.
- It stores lyrics locally in an editable, human-readable format.
- It includes a Studio for fixing timing, music breaks, and missing lyrics.
- It works without an account, cloud storage, or audio recording.

## Product principles

- **Local first.** Player data, lyrics, edits, and logs stay on the machine.
- **Follow, do not own.** Playback control is limited to play/pause/seek on the user's
  existing player.
- **No audio capture.** Only metadata and position are read.
- **One working text per song.** No on-disk version history; only the first LRCLIB text is
  preserved as the original.
- **Explicit saving.** The Studio never silently writes changes.
- **Graceful degradation.** A player without position or seek support still gets readable
  lyrics and a usable editor.
- **Quiet player, capable editor.** The karaoke view stays minimal; the Studio carries the
  complexity.
- **English first, Dutch included.** English is the source language; the UI can switch to
  Dutch.

## Current state

Implemented and exercised locally:

- Rust domain, MPRIS/SMTC providers, LRCLIB lookup, versioned library, watcher, and local API.
- Player, library browser, Studio, Dutch/English catalogs, offset support, and error states.
- Security headers, local-only validation, bounded JSON bodies, log rotation, and tests.
- Browser fallback and a Tauri 2 desktop shell that starts the same server and loads the
  built UI in a native window.

See `README.md` for usage and `docs/testing.md` for evidence and known gaps.

## Release readiness

- Release acceptance run in development, followed by finetuning.
- Real Windows SMTC run with a media application before a Windows release.
- Packaged Tauri builds with the frontend embedded or shipped as a resource, so the app runs
  from any working directory and without a repository checkout.
- Linux and Windows release builds in CI, with WebKitGTK/GTK development packages installed
  on Linux runners.
- Semantic version tags, a changelog, and GitHub Releases with downloadable binaries.
- Linux bundle formats (`.deb`, AppImage, or both) and Windows code signing if distribution
  grows beyond local use.
- Optional update notification against GitHub Releases; report only, never install silently.
- macOS only when a real test machine is available.

## Omarchy distribution (later)

Nico Karaoke is a normal desktop app. Nothing in the product needs Qt or a rewrite to run on
Omarchy; this work is packaging and desktop integration, not a port. Treat it as a later
distribution step after release acceptance and the Windows run.

- Embed the web assets, or install them as a package resource. The executable-relative
  fallback already works from any working directory, but a bundled resource path and a real
  package still need a build.
- Add an Arch package (PKGBUILD) that installs the binary, the web assets, a `.desktop` entry,
  and an icon. Prefer a small release tarball over a Rust/Node toolchain build.
- Align the `.desktop` filename, icon name, and actual window class so the launcher and
  Hyprland match the window to the app. Verify the class with `hyprctl clients`:
  `enableGTKAppId` is enabled, but the window class may still follow the binary name.
- Single-instance behaviour is already in place; verify that a packaged second launch focuses
  the existing window.
- Follow the Omarchy theme when it is present: read the current theme's `colors.toml` and
  generation file, pass the palette to the web UI, and keep the current palette as fallback.
  Check contrast for muted text and danger colours instead of copying theme values blindly.
- Optional Omarchy conventions: default language from the Omarchy language file, a `?`
  shortcut overlay, remembered window size, and Hyprland corner rounding.
- Release through `omacom/omarchy-pkgs` with a tagged GitHub release and checksums, and test
  the installed app from the launcher under Hyprland rather than only from a terminal.

## Product follow-ups

- Library sort options beyond the current most-recently-updated order.
- Import/export of local `.lrc` files and LRCGET libraries.
- Show the app version in the UI.
- An optional "open logs" action for support.
- Browser end-to-end tests for idle, playing, library, Studio, and save flows.
- A repeatable 60 fps budget check for very long songs and long lines.
- Community translation workflow: document adding a catalog and registering a language.
- Word-level synchronisation and the Studio word editor (see below).
- Optional local-only remote control from a phone, only after an explicit security design.

## Later product ideas

### Word-level synchronisation

- Read LRCLIB word-sync data in addition to line-synced lyrics.
- Store an optional word list per line and render each word with its own fill.
- Add a Studio word mode: split, distribute, tap, and nudge words for one line.
- Keep line-level lyrics as the fallback; word data is never required.
- Treat the word editor as a separate, serious phase.

### Share lyrics with LRCLIB

- Show whether an item came from LRCLIB, is local-only, or was shared before.
- Add an explicit confirmation flow with exact metadata and text.
- Solve LRCLIB's proof-of-work challenge in the background with progress.
- Publish only after confirmation; handle rate limits and failures cleanly.
- Add a report action for incorrect existing lyrics.
- Never publish automatically and never use real user lyrics in tests.

### Other

- Enhanced LRC import/export.
- Theme tokens for the web UI without depending on a specific desktop theme.
- Additional translations.

## Decisions

- **Backend:** Rust with Axum, Tokio, Serde, and Reqwest.
- **Frontend:** Vite and strict TypeScript without a UI framework; revisit only if the plain
  implementation becomes unwieldy.
- **Layout:** Cargo workspace with core, platform, server, and desktop crates, plus `web/`.
- **Desktop:** Tauri 2 is a shell around the same local server; the browser fallback remains
  first-class.
- **Storage:** clean, versioned `.lrc` plus `.json` files; no Python-library migration.
- **Local security:** loopback bind, `Host`/`Origin` validation, bounded JSON, no telemetry,
  no audio capture, and no automatic publishing.
- **Player scrolling:** centre the active lyric with native viewport scrolling, not a CSS
  transform; WebKitGTK at fractional scaling visibly jitters transformed text.
- **Player chrome:** the top bar and transport stay visible while playing; there is no
  auto-hide.
- **Licence:** MIT for the source code, documentation, and app icon; the head mark is a
  reserved personal brand asset (see `NOTICE`).

## Open questions

- Whether to generate the Rust/TypeScript API contract from one source instead of keeping
  `web/src/api.ts` in sync by hand.
- Whether release builds start with plain binaries or go straight to bundled Tauri artifacts.
- Whether a Python-era library importer is ever worth building.

## Explicit non-goals

- Recording, cloning, or enhancing audio.
- Downloading or streaming the song itself.
- A built-in media player.
- A cloud account or sync service.
- Automatic public publishing of lyrics.
- Telemetry or analytics.
- Backwards compatibility with the Python app's library or cache.
- A plugin system.
