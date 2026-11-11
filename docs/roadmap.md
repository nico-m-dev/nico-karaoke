# Nico Karaoke — roadmap

Status: draft for the Rust/TypeScript rewrite.
Updated: 2026-11-11.

This file defines **what the product is** and which behaviour must survive the rewrite. It is
not the execution plan; features move from here into phased checklists in `docs/TASKS.md`.

Priority labels:

- **[MVP]** — required for the rewrite to replace the current app.
- **[Next]** — valuable directly after the MVP, or needed for a usable release.
- **[Later]** — deliberately outside the first rewrite.

## What makes this karaoke app

Nico Karaoke is a local lyrics follower, not a media player:

- It follows the track already playing in another application.
- It shows line-synchronised lyrics with a green progress line in the centre.
- It stores lyrics locally in an editable, human-readable format.
- It includes a Studio for fixing timing, music breaks, and missing lyrics.
- It works without an account, without cloud storage, and without recording audio.

## Product principles

- **Local first.** Player data, lyrics, edits, and logs stay on the machine.
- **Follow, do not own.** Playback control is limited to play/pause/seek on the user's
  existing player.
- **No audio capture.** Only metadata and position are read.
- **One working text per song.** No version history on disk; only the first LRCLIB text is
  preserved as the original.
- **Explicit saving.** The Studio never silently writes changes.
- **Graceful degradation.** A player without position or seek support still gets readable
  lyrics and a usable editor.
- **Quiet player, capable editor.** The karaoke view stays minimal; the Studio carries the
  complexity.
- **English first, Dutch included.** The source language is English; the UI can switch to
  Dutch.

## Feature inventory — current behaviour

### A. Media tracking and playback control

- **[MVP]** Detect available players on Linux through MPRIS.
- **[MVP]** Detect available media sessions on Windows through SMTC.
- **[MVP]** Read title, artist, album, duration, playback state, position, track identity,
  and seek capability.
- **[MVP]** Poll the player a few times per second, not per animation frame.
- **[MVP]** Interpolate position in the UI between polls for smooth 60 fps movement.
- **[MVP]** Choose between multiple open players; remember the selected player during the
  session.
- **[MVP]** Fall back to the playing session when the selected one disappears.
- **[MVP]** Send play/pause, relative seek, and absolute seek-to requests.
- **[MVP]** Report unsupported seek or missing position clearly instead of failing silently.
- **[MVP]** Keep working when a player exposes metadata but no position: show plain,
  scrollable lyrics.
- **[MVP]** Never record audio.

### B. Lyrics lookup and cache

- **[MVP]** Query LRCLIB without an API key.
- **[MVP]** Clean track titles before lookup: remove video decorations, `feat.` fragments,
  `- Topic`, VEVO-style channel suffixes, and split `Artist - Title` when the channel name
  is not the real artist.
- **[MVP]** Lookup ladder: exact `get` with album, exact `get` without album, then `search`.
- **[MVP]** Prefer a synchronised result over plain text, even when the plain text came from
  the exact `get`.
- **[MVP]** When searching, only accept candidates within roughly five seconds of the known
  duration; sort remaining candidates by distance.
- **[MVP]** Deduplicate candidates by LRCLIB id or track identity.
- **[MVP]** Store found lyrics in the local library immediately.
- **[MVP]** Cache misses for one hour so a temporary failure does not cause a request storm.
- **[MVP]** Prevent duplicate concurrent lookups for the same track.
- **[MVP]** Discard a late lookup result when the track changed in the meantime.
- **[MVP]** Preserve the first fetched LRCLIB text as the editable item's original.
- **[MVP]** Keep source, LRCLIB id, fetch time, update time, and user offset with the item.
- **[Next]** Use a clear, identifiable User-Agent and handle rate limits politely.
- **[Later]** Read and use word-synchronised LRCLIB records.

### C. Player view

- **[MVP]** Idle screen with a calm "play music" message and a library entry point.
- **[MVP]** Show title and artist for the followed track.
- **[MVP]** Keep the active line on the centre green line.
- **[MVP]** Fill the active line green proportionally until the next line or music marker.
- **[MVP]** Render upcoming lines lighter and past lines dimmed.
- **[MVP]** Scroll smoothly without a server request per frame.
- **[MVP]** Treat a `♪` line as an instrumental break: it does not fill slowly, and it stops
  the previous line's fill.
- **[MVP]** Fade the top and bottom bars after inactivity; restore them on mouse movement.
- **[MVP]** Space toggles play/pause on the followed player.
- **[MVP]** `F` toggles fullscreen.
- **[MVP]** Show a player picker when more than one player is available.
- **[MVP]** Show a readable plain-text layout when lyrics are not synchronised.
- **[MVP]** Apply the saved global lyrics offset.
- **[Next]** Honour reduced-motion preferences for scrolling and fades.
- **[Next]** Ensure the player remains usable with keyboard only and with screen readers.

### D. Library

- **[MVP]** List all saved lyrics with title, artist, album, duration, source, sync state,
  line count, update time, and original availability.
- **[MVP]** Search by title, artist, or album.
- **[MVP]** Open any item in the Studio, whether or not it is currently playing.
- **[MVP]** Delete an item with confirmation, removing both the `.lrc` and `.json` files.
- **[MVP]** Offer YouTube and Spotify search links per item.
- **[MVP]** Allow a deleted song to be fetched again automatically when it plays later.
- **[MVP]** Refresh the library list without reloading the app.
- **[Next]** Sort options beyond the default most-recently-updated order.
- **[Later]** Import/export local LRC files or LRCGET lyrics files.

### E. Studio editor

- **[MVP]** Dedicated Studio page per song, opened by library key or by the currently playing
  track.
- **[MVP]** Header with back link, title, artist, source, original availability, play/pause,
  current time, duration, save button, and dirty indicator.
- **[MVP]** Horizontal, zoomable timeline with time ruler, one marker per line, and a moving
  playhead.
- **[MVP]** Zoom range of roughly 5–120 px/s plus "fit to window".
- **[MVP]** Click a marker to select it and seek to its time.
- **[MVP]** Drag a marker to change its time.
- **[MVP]** Click the ruler to seek.
- **[MVP]** Show labels only where they fit, with tooltips.
- **[MVP]** Editable line list with time, text, play action, and delete action.
- **[MVP]** Inline editing of time and text; re-sort after a time edit.
- **[MVP]** Add a line at the current playback position or after the selected line.
- **[MVP]** Insert a music marker at the current playback position; prevent duplicate markers
  within 0.1 s.
- **[MVP]** Inspector nudge buttons for −0.5, −0.1, −0.01, +0.01, +0.1, +0.5 seconds.
- **[MVP]** "Take current time" for the selected line, also when seeking is unsupported.
- **[MVP]** Preview the selected line: pause, seek 1.5 s before it, play, and pause 1.5 s
  after it.
- **[MVP]** Toggle a line between lyric and music marker.
- **[MVP]** Delete a line; undo covers it during the editing session.
- **[MVP]** Tap mode: start the song at zero, count down, tap the start of each line, and
  return to the editor with new times.
- **[MVP]** Configurable tap correction, default −0.2 s, to compensate for reaction time.
- **[MVP]** Backspace works during tapping; a wrong tap can be fixed on the timeline.
- **[MVP]** Raw LRC panel for viewing and replacing the whole text.
- **[MVP]** Fetch from LRCLIB for a library item that is not currently playing.
- **[MVP]** Restore the saved original with confirmation.
- **[MVP]** Explicit save; warn before leaving with unsaved changes.
- **[MVP]** Undo stack of about 30 editor changes; no on-disk version history.
- **[MVP]** Keyboard shortcuts: space, arrows for nudging, Shift/Alt modifiers, `Ctrl+S`,
  `Ctrl+Z`.
- **[MVP]** Migrate legacy `[end:mm:ss.xx]` tags into `♪` markers where there is a real gap,
  and stop writing end tags.
- **[MVP]** Keep old `[end:]` tags readable until the file is saved again.
- **[Next]** Word-level editing mode for a single selected line.
- **[Later]** Import/export enhanced LRC.

### F. Persistence and migration

- **[MVP]** Keep one human-readable `.lrc` file and one `.json` metadata file per song.
- **[MVP]** Treat the `.lrc` file as the source of truth after a manual edit.
- **[MVP]** Do not let an offset-only or metadata-only save overwrite newer manual text.
- **[MVP]** Write files atomically through a temporary file and rename.
- **[MVP]** Derive a stable record key from normalised artist and title.
- **[MVP]** Use readable slugged file names with a short key suffix.
- **[MVP]** Store `version`, `key`, metadata, `source`, `synced`, `offset`, timestamps,
  `lrclib_id`, and the original text.
- **[MVP]** Import the legacy `lyrics.json` cache once, then rename it to prevent a second
  import.
- **[MVP]** Store data in the platform data directory, with an environment override.
- **[MVP]** Write a log file in the data directory for windowless builds.
- **[MVP]** Remove stale files when a record's slug changes.
- **[Next]** Back up before any future schema migration.
- **[Later]** Optional portable library directory.

### G. Localisation and accessibility

- **[MVP]** English and Dutch catalogs with identical keys.
- **[MVP]** English source strings; Dutch translation kept complete.
- **[MVP]** Persist the chosen language across reloads.
- **[MVP]** Default to English for non-Dutch browsers and to Dutch for Dutch browsers until a
  choice is made.
- **[MVP]** Set the document language and title per locale.
- **[MVP]** Translate API error codes in the frontend.
- **[MVP]** Use `Intl` for visible dates and decimals.
- **[MVP]** Tests that fail on missing keys, unknown keys in markup, or unknown keys used in
  code.
- **[Next]** Document how to add a language.
- **[Later]** Additional community translations.

### H. Local server, security, and operations

- **[MVP]** Serve the UI and API from a local HTTP server.
- **[MVP]** Bind to `127.0.0.1` by default.
- **[MVP]** Support `--port`, `--host`, `--browser`, and `--no-open`.
- **[MVP]** Move to the next free port when the requested port is busy.
- **[MVP]** Reject unexpected `Host` and `Origin` headers.
- **[MVP]** Require `application/json` for API writes and enforce a body-size limit.
- **[MVP]** Protect static file serving against path traversal.
- **[MVP]** Avoid caching API responses.
- **[MVP]** Log startup, media failures, lookup failures, and library errors without leaking
  lyric bodies.
- **[MVP]** Open a native window when available, otherwise fall back to the default browser.
- **[Next]** Structured logs with a configurable level.
- **[Next]** Optional "open logs" action for support.
- **[Later]** Local-only remote control from a phone on the same network, if ever wanted.
  This requires an explicit security design and is not part of the rewrite.

### I. Packaging and distribution

- **[MVP]** Run from source with one documented command for backend and frontend.
- **[MVP]** Produce a single Linux binary and a single Windows executable.
- **[MVP]** Bundle static frontend assets into the binary or alongside it as one artifact.
- **[MVP]** Include the app icon and version metadata.
- **[MVP]** Hide the console window on Windows; log to the data directory.
- **[MVP]** GitHub Actions matrix for Linux and Windows: lint, test, build, upload artifact.
- **[Next]** Tauri-based desktop shell with the same web UI.
- **[Next]** Git tags with semantic versions and a changelog.
- **[Next]** GitHub Releases with bare binaries, not archive-only artifacts.
- **[Next]** Show the version in the app.
- **[Later]** Update check against the GitHub API; report only, never install silently.
- **[Later]** Code signing for Windows.
- **[Later]** AppImage, `.deb`, or other Linux packaging if users ask.
- **[Later]** macOS, only after a real test machine is available.

### J. Testing and quality

- **[MVP]** Rust unit tests for LRC parsing, timestamp formatting, title cleaning, key
  generation, and the lookup ladder with a mocked client.
- **[MVP]** Rust tests for library load/save/delete, atomic writes, manual-file precedence,
  old-end migration, and legacy import.
- **[MVP]** API tests with a fake media provider and fake lyrics client.
- **[MVP]** Frontend unit tests for time formatting, active-line lookup, fill calculation,
  and i18n key usage.
- **[MVP]** A fake-player/fake-LRCLIB development mode for manual UI work.
- **[MVP]** `cargo fmt`, `cargo clippy -D warnings`, TypeScript strict, linter, and formatter.
- **[Next]** Browser end-to-end tests for idle, playing, library, Studio, and save flows.
- **[Next]** Manual acceptance matrix on Linux (MPRIS) and Windows (SMTC) with real players.
- **[Later]** Performance budget check for 60 fps lyrics with long songs and long lines.

## Rewrite-specific goals

These are not user features; they are reasons for the rewrite and constraints on it.

- **[MVP]** Replace the single large Python server file with small Rust modules and clear
  domain boundaries.
- **[MVP]** Make media, lyrics lookup, storage, and HTTP independently testable.
- **[MVP]** Keep the web UI but move it to strict TypeScript with a build step.
- **[MVP]** Keep the existing library files usable; do not force users to retime their songs.
- **[MVP]** Make platform differences explicit through traits and conditional compilation,
  not runtime string checks scattered through the code.
- **[MVP]** Keep the app runnable as a plain local web app before Tauri exists.
- **[Next]** Share API types between Rust and TypeScript from one source.
- **[Next]** Make a future desktop shell a packaging concern, not an architecture rewrite.

## Later product ideas

These come from the previous roadmap and remain valid, but are not part of the MVP.

### Word-level synchronisation

- Read LRCLIB `lyricsfile` and `hasWordSync` in addition to `syncedLyrics`/`plainLyrics`.
- Store an optional `words: [{ text, start, end? }]` list per line.
- Read and write enhanced LRC word tags.
- Render the active line from word spans, each with its own fill.
- Add a word mode in the Studio for one selected line: split, distribute, tap, and nudge
  words.
- Keep line-level lyrics as the fallback; never require word data.
- Treat the word editor as a separate, serious phase.

### Share lyrics with LRCLIB

- Show whether an item came from LRCLIB, is local-only, or was shared before.
- Add an explicit confirmation flow with title, artist, album, duration, and the exact text.
- Solve LRCLIB's proof-of-work challenge in the background with progress.
- Publish through the experimental API only after explicit confirmation.
- Handle missing album metadata, rate limits, and failures cleanly.
- Add a report action for incorrect existing lyrics.
- Never publish automatically and never use real user lyrics in tests.

### Distribution and updates

- Semantic versions and tags.
- Changelog per release.
- GitHub Releases with downloadable binaries.
- Version display in the app.
- Optional update notification.
- Manual replacement remains acceptable while the app is portable.

### Other

- Import/export of local LRC and LRCGET lyrics files.
- Theme tokens for the web UI, without making the app depend on a specific desktop theme.
- Additional translations.
- macOS support when hardware is available.

## Explicit non-goals

- Recording, cloning, or enhancing audio.
- Downloading or streaming the song itself.
- A built-in media player.
- A cloud account or sync service.
- Automatic public publishing of lyrics.
- Telemetry or analytics.
- A plugin system before the core rewrite is stable.

## Open decisions before implementation

These need a yes/no before scaffolding starts:

1. **Rust web stack:** Axum + Tokio + Serde + Reqwest.
2. **Frontend:** Vite + strict TypeScript, no UI framework for the player; consider a small
   framework only for the Studio if plain TypeScript becomes unwieldy.
3. **Repository layout:** Cargo workspace with domain, platform, and server crates, plus a
   `web/` package. Tauri added later as a separate shell crate.
4. **Compatibility:** keep the existing `.lrc` + `.json` library format and add a one-time
   legacy import, rather than inventing a new database.
5. **First milestone:** browser-first local server with the full player, library, and Studio
   flows; Tauri packaging after that works.
6. **Licence:** MIT, inherited from the original project.

## Suggested first phases

Detailed checklists belong in `docs/TASKS.md`. The intended order is:

1. **Foundation** — workspace, TypeScript build, config, logging, CI skeleton, test harness.
2. **Domain and storage** — LRC parser, title cleaning, keys, library format, migration.
3. **Lyrics lookup** — LRCLIB client, lookup ladder, cache, mocks.
4. **Media providers** — shared trait, fake provider, Linux MPRIS, Windows SMTC.
5. **HTTP API** — state, library, control, lyrics, Studio endpoints, security.
6. **Player UI** — idle, lyrics, fill, scrolling, player picker, fullscreen, i18n.
7. **Library UI** — search, list, edit/delete, links.
8. **Studio UI** — timeline, line list, inspector, preview, tap, raw LRC, undo.
9. **Hardening** — tests, accessibility, error states, logs, manual player matrix.
10. **Packaging** — release builds, CI artifacts, Tauri shell, versioning.
