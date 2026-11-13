# Testing and acceptance

This document records what is verified automatically, what has been checked on a real
desktop, and what still needs a human. Keep measurements tied to the machine and build they
came from; do not present a headless result as a guarantee for every machine.

## Automated checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo check -p nico-karaoke-platform --all-targets --target x86_64-pc-windows-msvc

npm run typecheck --prefix web
npm run lint --prefix web
npm run format:check --prefix web
npm test --prefix web
npm run build --prefix web
```

The Windows platform crate is compile-checked on Linux. A full Windows workspace check needs
the MSVC linker, so CI runs it on `windows-latest`.

GitHub Actions runs the Rust checks on Linux and Windows and the web checks on Linux. The manual
**Build test artifacts** workflow produces unsigned Linux and Windows test builds with the
frontend embedded in the executable; the Windows build is the first real SMTC test target before
a public release.

## Server hardening

- The server only accepts loopback `Host` values and local `Origin` values. Foreign requests
  return `403` with `forbidden_host` and `no-store`.
- API writes require `application/json`; malformed JSON, an unsupported content type, and
  oversized bodies return stable JSON error codes.
- Static serving rejects traversal attempts; tests cover encoded and plain `..` paths.
- Every response, including errors and static files, carries `no-store` plus these headers:
  `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`,
  `Cross-Origin-Opener-Policy: same-origin`, and a self-only content security policy.
- The CSP allows inline styles because the player and timeline position elements are positioned with CSS
  custom properties. A Chromium load of the player and Studio reported no CSP violations.
- The app never binds to a public interface by default and never sends telemetry.

## Lyrics round-trip

- Metadata tags such as `[ar:]`, `[ti:]`, and `[al:]` survive a parse and Studio save. The
  parser deliberately drops `[offset:]` after applying it to the timestamps, so saving does
  not shift the file twice.
- Untimed, non-metadata lines in a mixed document are preserved on save but are not shown as
  timed lyrics.
- Rust parser/serializer tests and the Studio serializer test cover both cases.

## Logs

`nico-karaoke.log` lives in the data directory. At startup, a file of at least 5 MiB is moved
to `.1`, older backups shift down, and at most three backups are kept. Rotation is a startup
policy: a single very long session can grow past the limit until the next start.

## Dependencies

`npm audit` reports no advisories, including development tooling. The ESLint toolchain pulled a
`minimatch` version affected by a denial-of-service advisory; `web/package.json` pins the
fixed `minimatch` through an `overrides` entry. Revisit that override when ESLint and
`typescript-eslint` update their dependencies.

## Accessibility

- Body text and metadata meet WCAG AA contrast on the dark background. Inactive lyric lines
  use large-text contrast (at least 3:1) and are still distinguishable from the active line.
- The song title is the level-1 heading while playing; the idle screen has one level-1
  heading of its own.
- Every control has an accessible name. Dynamic status uses `role="status"` or `aria-live`.
- Focus is visible on buttons, links, inputs, and summaries. The library dialog traps focus
  and restores the opener. Tap mode marks the background inert and traps focus.
- Reduced-motion preferences disable transitions.

Remaining manual checks: a real screen-reader pass on the player, library, and tap mode, and
a keyboard-only pass on the timeline. Timeline markers are pointer targets; the line list
provides the keyboard path for selecting, seeking, nudging, and retiming.

If the terminal shows `Couldn't connect to accessibility bus` or `Can't connect to a11y bus`,
the session AT-SPI bus is stale: the socket exists but has no live listener. Restart it with
`systemctl --user restart at-spi-dbus-bus.service` and relaunch the app. Do not set
`NO_AT_BRIDGE=1` as a default workaround, because that turns the accessibility bridge off.

## Idle art and accent colours

- Five accent palettes live as CSS variables on `<html data-accent>`, so the player, lyrics
  highlight, controls, and Studio use one choice. The choice persists as `nico.accent`; lime
  is the default.
- Every accent colour meets AA (4.5:1) for normal text against the page background and the
  accent-tinted panel surface used by primary buttons; the lowest is magenta at 5.0:1, and
  cyan, lime, and orange exceed 6.9:1. Inactive lyric lines keep the existing large-text
  treatment.
- The accent dots are buttons with a translated accessible name and `aria-pressed`. The idle
  artwork is decorative and marked `aria-hidden`.
- `web/src/accent.test.ts` covers detection, unsupported values, storage failures, persistence,
  and applying the root attribute. The translation test covers the English and Dutch key sets;
  the label values themselves are checked by review.
- Checked in Chromium on 2026-11-13 at 1440×900, 1280×600, 1280×720, 900×640, 844×560,
  844×390, 700×600, 390×844, and 320×568, in English and Dutch. Every accent swaps the
  instrument, the choice survives a reload, the Studio follows it on load and when another
  tab changes it, and the idle copy stays legible over all five artworks. The short/wide split
  stage was checked at 1280×600, 900×640, 844×560, 700×600, and 844×390. A pass on WebKitGTK
  (Tauri) and Windows WebView2 remains.
- The stored accent is applied by the entry module. The CSP forbids an inline pre-paint script,
  so a first load can briefly show the lime default before the module runs.

## Performance

Measured with headless Chromium (software rendering) against a local server and a fake
three-hour track:

- Player with 2000 lines: first render about 0.6 s, 6,056 DOM nodes, about 53 frames in one
  second.
- Studio with 2000 lines: first render about 2.0 s, 24,109 DOM nodes including 2000 timeline
  markers; scrolling the line list takes about 13 ms. Normal songs are far below this.
- A 1505-character line wraps inside the player instead of creating horizontal page
  overflow. In the Studio the line stays inside its input and scrolls horizontally.

These are exploratory budgets, not hard guarantees. The player measurements predate the
native-scroll centring; re-measure when the fixed 60 fps budget is picked up.

## Native window

The Tauri 2 shell loads the built UI from the local Axum server, which it starts itself.

- Verified on Linux: window title, player, Studio, `/api/health`, port fallback when `8770` is
  occupied, shared data directory and log file, and clean shutdown with no orphaned listener.
- External YouTube/Spotify links are handed to the system browser instead of opening a second
  webview.
- The shell starts from an arbitrary working directory when the frontend is placed next to the
  binary or under `../share/nico-karaoke/web`; checked from a temporary directory with a copied
  binary and assets.
- HMR starts without a prior `web/dist` build when `NICO_KARAOKE_WEBVIEW_URL` is set; the
  startup path was exercised. A full HMR session with a running Vite server remains for release
  acceptance.
- A non-loopback `--host` is rejected with exit code 2 before the listener binds.
- A missing frontend build shows a GTK error dialog when the app is launched without a
  terminal; a terminal launch prints the same message and exits with code 2.
- A second launch exits instead of starting a second window, server, and watcher.
- The browser fallback and `cargo run -p nico-karaoke-server` remain unchanged.
- Tauri embeds the app icon as the default window icon. Hyprland's window decoration on
  Omarchy has no icon area, so it does not appear in the title bar; launchers and taskbars get
  it through the packaged `.desktop` entry and installed icon.
- Not verified here: the native window on Windows, bundled/assets packaging, the launcher
  icon on an installed package, and the release checklist. Those remain for packaging.

## Player rendering

The player centres the active lyric with native viewport scrolling; it does not move the track
with a CSS transform. This changed after WebKitGTK 4.1 at 1.25 fractional scaling made
transformed lyric text visibly jump. A fullscreen WebKitGTK test view with a fake player
confirmed that the active line stays centred and line changes scroll without the jump. The top
bar and transport stay visible while playing; the auto-hide was removed because it made the
controls intermittently unreachable.

A wrapped active line measures its visual line boxes and clips the green fill in reading order
instead of applying one horizontal slice across the whole block. Headless Chromium at 1000 px
and 620 px confirmed that the second visual line stays unfilled until the sweep reaches it, also
after a resize, and that a past line turns fully green. The clip computation has unit tests in
`web/src/player.test.ts`.

The Studio transport sends an explicit `play` or `pause` based on the intended state instead of
a blind toggle, and shows that state immediately while a poll may still describe the previous
one. It also queues a single click that arrives while a control request is in flight instead of
swallowing it (a double-click still counts as one toggle). The control stays usable while the
same player is followed, even when a poll briefly drops `can_play`, reports a changed track id,
or returns another lyrics key; only a disappearing player or a real follow-target change
disables it. A seek guard holds the timeline playhead at the requested position until the
player reports it, so a stale `getNow()` response no longer makes the playhead jump back and
forth. The seek and transport state machines have unit tests in `web/src/studio/seek.test.ts`
and `web/src/studio/transport.test.ts`. A headless Chromium run against a real `mpv` session
confirmed that one click flips the control immediately and the player follows, that a
double-click counts once, that a simulated `can_play`/metadata/key drop on the same player no
longer swallows the click, and that a marker seek moves the playhead forward without a backward
step. A full release acceptance pass on the Studio remains.

## README screenshots

The screenshots in `docs/screenshots/` were captured against an isolated demo data directory.
The song titles, artist names, and lyrics are fictional and written for the captures; no
personal data or third-party artwork is shown. The idle, player, library, and Studio screens
were refreshed after the neutral surface and accent palette change.

## Release build

- On 2026-11-13 the 1.0.0 desktop binary was built with
  `cargo build --release --locked -p nico-karaoke-desktop` after `npm run build --prefix web`.
- The staged portable archive (`NicoKaraoke-1.0.0-linux-x86_64.tar.gz`) contains `NicoKaraoke`,
  `README.md`, `LICENSE`, and `CHANGELOG.md`.
- Running `NicoKaraoke` from an empty directory with an empty data directory reported
  `version: 1.0.0` from `/api/health`, served the idle screen, and returned the embedded
  `idle-lime.webp` as `image/webp` with `X-Content-Type-Options: nosniff`.
- `v1.0.0-rc.1`: both CI builds passed, but the draft step looked for
  `docs/releases/v1.0.0-rc.1.md` instead of the package's `v1.0.0.md`. The downloaded Linux
  artifact was still verified: version 1.0.0, idle screen, embedded WebP with the right content
  type. The notes lookup was fixed and `rc.2` rebuilt.
- `v1.0.0-rc.2`: the release workflow passed and created a draft release with both archives and
  `SHA256SUMS.txt`; both checksums verified. The Linux archive from the draft was extracted and
  started from an empty directory: version 1.0.0, `/api/health` version 1.0.0, and the embedded
  `idle-lime.webp` served as `image/webp`. The Windows archive contains `NicoKaraoke.exe`,
  `README.md`, `LICENSE`, and `CHANGELOG.md`; the real Windows acceptance run is the remaining
  step.

## Manual player matrix

| Area | Linux | Windows |
| --- | --- | --- |
| Discovery and metadata | **Verified** with real `mpv` 0.41 and `mpv-mpris`: title, length, and position; artist/album depend on the file tags | Compile-checked SMTC crate only |
| Position and duration | **Verified** with real `mpv` | Not run |
| Play, pause, seek | **Verified** with real `mpv` | Not run |
| Multiple players and selection | **Verified** with two stateful fake MPRIS players | Not run |
| Player disappearing or no player | **Verified** with fake players | Not run |
| Lyrics lookup and library | **Verified** with fake LRCLIB responses and live LRCLIB | Not run |

Linux checks: a real `mpv` session reported title, length, advancing position, `CanSeek`, and
`CanPlay`; play, pause, and an absolute seek all reached the player. Two fake MPRIS players
exercised selection and fallback. A stateful fake player covered paused, missing position,
slow seek, and disappearing-player cases.

Windows SMTC still needs a real Windows machine with a media application before packaging is
called done.
