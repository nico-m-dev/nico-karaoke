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

## Server hardening

- The server only accepts loopback `Host` values and local `Origin` values. Foreign requests
  return `403` with `forbidden_host` and `no-store`.
- API writes require `application/json`; malformed JSON, an unsupported content type, and
  oversized bodies return stable JSON error codes.
- Static serving rejects traversal attempts; tests cover encoded and plain `..` paths.
- Every response, including errors and static files, carries `no-store` plus these headers:
  `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`,
  `Cross-Origin-Opener-Policy: same-origin`, and a self-only content security policy.
- The CSP allows inline styles because the player and timeline position elements through CSS
  custom properties. A Chromium load of the player and Studio reported no CSP violations.
- The app never binds to a public interface by default and never sends telemetry.

## Logs

`nico-karaoke.log` lives in the data directory. At startup, a file of at least 5 MiB is moved
to `.1`, older backups shift down, and at most three backups are kept. Rotation is a startup
policy: a single very long session can grow past the limit until the next start.

## Dependencies

`npm audit --omit=dev` reports no runtime advisories. The ESLint toolchain pulled a
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

## Performance

Measured with headless Chromium (software rendering) against a local server and a fake
three-hour track:

- Player with 2000 lines: first render about 0.6 s, 6,056 DOM nodes, about 53 frames in one
  second.
- Studio with 2000 lines: first render about 2.0 s, 24,583 DOM nodes including 2000 timeline
  markers; scrolling the line list takes about 13 ms. Normal songs are far below this.
- A 1505-character line wraps inside the player instead of creating horizontal page
  overflow. In the Studio the line stays inside its input and scrolls horizontally.

These are exploratory budgets, not hard guarantees. A fixed 60 fps performance gate remains a
later roadmap item.

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
