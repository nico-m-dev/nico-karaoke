# Nico Karaoke — roadmap

Future work and open decisions. Product behaviour, architecture, and settled decisions live in
`README.md` and `AGENTS.md`; test evidence lives in `docs/testing.md`.

Updated: 2026-11-13.

## Release readiness

- Release acceptance run in development, followed by finetuning.
- Real Windows SMTC run with a media application before a Windows release.
- Packaged Tauri builds with the frontend embedded or shipped as a resource, so the app runs
  from any working directory and without a repository checkout.
- Linux and Windows release builds in CI, with WebKitGTK/GTK development packages installed on
  Linux runners.
- Semantic version tags, a changelog, and GitHub Releases with downloadable binaries.
- Linux bundle formats (`.deb`, AppImage, or both) and Windows code signing if distribution
  grows beyond local use.
- Optional update notification against GitHub Releases; report only, never install silently.
- macOS only when a real test machine is available.

## Later distribution

### Omarchy / Arch

- Add an Arch package (PKGBUILD) that installs the binary, web assets, a `.desktop` entry, and
  an icon. Prefer a small release tarball over a Rust/Node toolchain build.
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

- Library sort options beyond the current order: the current track first, then most recently
  updated.
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
