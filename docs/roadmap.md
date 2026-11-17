# Nico Karaoke — roadmap

Future work and open decisions. Product behaviour and settled decisions live in `README.md` and
`AGENTS.md`; implementation notes live in `docs/development.md`; test evidence lives in
`docs/testing.md`.

Updated: 2026-11-17.

## After 1.0.0

- Windows code signing (Authenticode) if SmartScreen warnings become a problem.
- A Linux `.deb` and an Arch package for package-managed installs; those update through the
  package manager and must not offer in-app updates.
- Verify the first updater-enabled release end to end on Linux and Windows, including a failed
  download, an offline start, and the restart.
- macOS only when a real test machine is available.

## Later distribution

### Omarchy / Arch

- Add an Arch package (PKGBUILD) that installs the binary, web assets, a `.desktop` entry, and
  an icon. Prefer a small release tarball over a Rust/Node toolchain build. Package-managed
  installs update through pacman, not the in-app updater.
- Embed the web assets or install them as a package resource. The executable-relative fallback
  already works; a bundled resource path and a real package still need a build.
- Align the `.desktop` filename, icon name, and actual window class so the launcher and
  Hyprland match the window to the app. Verify the class with `hyprctl clients`.
- Verify that a packaged second launch focuses the existing window; single-instance behaviour
  is already in place.
- Follow the Omarchy theme when present: read the current `colors.toml` and generation file,
  pass the palette to the web UI, and keep the current palette as fallback. Check contrast for
  muted text and danger colours instead of copying theme values blindly.
- Optional Omarchy conventions: default language from the Omarchy language file, a `?` shortcut
  overlay, remembered window size, and Hyprland corner rounding.
- Release through `omacom/omarchy-pkgs` with a tagged GitHub release and checksums, and test
  the installed app from the launcher under Hyprland rather than only from a terminal.

## Product follow-ups

- Import/export of local `.lrc` files and LRCGET libraries.
- Show the app version in the UI.
- An optional "open logs" action for support.
- Browser end-to-end tests for idle, playing, library, Studio, and save flows.
- A repeatable 60 fps budget check for very long songs and long lines.
- Community translation workflow: document adding a catalog and registering a language.
- Optional local-only remote control from a phone, only after an explicit security design.

## Maybe, not decided

These are ideas only; nothing here is planned or promised.

- A read-only second-screen player over the local network, so a TV, tablet, or phone can show the
  same lyrics as the host. Multiple viewers would connect to the same session. It would need an
  explicit opt-in, a pairing token, and a strictly read-only API; Chromecast, AirPlay, and cloud
  receivers are out of scope. Screen mirroring or a cable remains the recommended setup for now.

## Advanced authoring (later)

The everyday workflow stays line-based, local, and simple. The ideas below are for people who
deliberately polish lyrics; they must sit in a clearly separated, opt-in place, never as an
extra button in normal playback or saving.

### Word-level synchronisation

- Read LRCLIB word-sync data in addition to line-synced lyrics.
- Store an optional word list per line and render each word with its own fill.
- Add a Studio word mode: split, distribute, tap, and nudge words for one line.
- Keep line-level lyrics as the default; playback, saving, and the normal Studio never require
  word data.
- Put the word editor behind an explicit advanced mode, not in the everyday editing flow.
- Treat the word editor as a separate, serious phase with its own acceptance criteria.

### Share lyrics with LRCLIB

Publishing is not a one-click action and not a feature for incidental listeners. A casual
upload of half-finished or mistimed text is not a gift to LRCLIB; it adds work for its
maintainers. Any future contribution flow therefore belongs in a dedicated advanced section
for users who take their saved lyrics seriously.

- Show whether an item came from LRCLIB, is local-only, or was shared before.
- Offer a **report incorrect lyrics** action before offering a duplicate upload, and check
  whether LRCLIB already has an entry for the song.
- Add an explicit confirmation flow with the exact metadata, text, and a diff against the
  existing LRCLIB entry.
- Require the user to review the exact payload and state that it is accurate; consider
  requiring line-synced lyrics for upload.
- Solve LRCLIB's proof-of-work challenge in the background with progress.
- Publish only after confirmation; handle rate limits and failures cleanly.
- Keep the normal app upload-free: nothing leaves the machine unless the user deliberately
  enters this flow.
- Never publish automatically, never bulk-upload, and never use real user lyrics in tests.

### Other

- Enhanced LRC import/export.
- Theme tokens for the web UI without depending on a specific desktop theme.
- Additional translations.
