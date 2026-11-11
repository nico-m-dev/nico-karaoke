# Nico Karaoke

Synchronised lyrics for whatever is playing in another application. The app reads the
current track and position from MPRIS on Linux or SMTC on Windows, fetches lyrics from
LRCLIB, and draws a line-by-line karaoke view in a local web interface.

This repository is a clean Rust and TypeScript rewrite of an earlier Python application. It
does not record audio, download tracks, or play video.

## Status

Foundation phase. The workspace, local server, typed frontend, and CI skeleton exist; media,
lyrics, library, and Studio behaviour are next.

See `docs/roadmap.md` for product scope and `docs/TASKS.md` for the current plan.

## Requirements

- Rust stable
- Node.js 22 or newer
- On Linux, a running media player that publishes MPRIS metadata (later phases)
- On Windows, an application that publishes SMTC metadata (later phases)

## Run locally

Build the frontend, then start the server:

```bash
npm install --prefix web
npm run build --prefix web
cargo run -p nico-karaoke-server
```

Open <http://127.0.0.1:8770/>. The server tries the next free port when `8770` is busy.

For frontend development, run the Vite server and the Rust server side by side:

```bash
cargo run -p nico-karaoke-server
npm run dev --prefix web
```

Vite proxies `/api` to the Rust server. Open the URL printed by Vite.

## Configuration

| Setting | Environment variable | Default |
| --- | --- | --- |
| Data directory | `NICO_KARAOKE_DATA` | platform local data directory |
| Frontend build | `NICO_KARAOKE_WEB` | `web/dist` |
| Log filter | `RUST_LOG` | server info, HTTP warnings |

Command-line options:

| Option | Meaning |
| --- | --- |
| `--host <addr>` | address to bind (default `127.0.0.1`) |
| `--port <port>` | first port to try (default `8770`) |
| `--browser` | force browser launching (wired in a later phase) |
| `--no-open` | start the server without opening anything |

## Checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run typecheck --prefix web
npm test --prefix web
npm run build --prefix web
```

## Repository layout

```text
crates/core/       domain logic and configuration
crates/platform/   MPRIS and SMTC boundary
crates/server/     local Axum server and static assets
web/               Vite + strict TypeScript UI
docs/              roadmap, tasks, and design notes
```

## Licence

MIT. See `LICENSE`.
