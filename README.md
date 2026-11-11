# Nico Karaoke

Synchronised lyrics for whatever is playing in another application. The app reads the
current track and position from MPRIS on Linux or SMTC on Windows, fetches lyrics from
LRCLIB, and draws a line-by-line karaoke view in a local web interface.

This repository is a clean Rust and TypeScript rewrite of an earlier Python application. It
does not record audio, download tracks, or play video.

## Status

Foundation phase. The workspace, local server, typed frontend, and CI skeleton exist; media,
lyrics, library, and Studio behaviour are next. Host/Origin validation and the full local-only
HTTP policy are scheduled for the API phase; the server currently binds to `127.0.0.1` by
default.

See `docs/roadmap.md` for product scope and `docs/TASKS.md` for the current plan.

## Requirements

- Rust stable
- Node.js 22.12 or newer (LTS recommended)
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

The default frontend directory is `web/dist` relative to the current working directory. Run
the server from the repository root, or set `NICO_KARAOKE_WEB` to an absolute or relative path.

For frontend development, run the Vite server and the Rust server side by side:

```bash
cargo run -p nico-karaoke-server
npm run dev --prefix web
```

Vite proxies `/api` to the Rust server. Open the URL printed by Vite.

TypeScript is pinned to the 6.0.x line because `typescript-eslint` currently supports
TypeScript `<6.1`. Revisit the pin when the lint toolchain supports TypeScript 7.

## Configuration

| Setting | Environment variable | Default |
| --- | --- | --- |
| Data directory | `NICO_KARAOKE_DATA` | platform local data directory |
| Frontend build | `NICO_KARAOKE_WEB` | `web/dist` |
| Log filter | `RUST_LOG` | server info, HTTP warnings |

Command-line options:

| Option | Meaning |
| --- | --- |
| `--host <addr>` | address to bind: an IP literal or `localhost` (default `127.0.0.1`) |
| `--port <port>` | first port to try (default `8770`; `0` picks a free port) |
| `--browser` | force browser launching (wired in a later phase) |
| `--no-open` | start the server without opening anything |
| `-h`, `--help` | show usage |
| `-V`, `--version` | show the version |

## Data

The app stores its library in the platform data directory, under `library/`. Each song has
two files:

- a human-readable `.lrc` file with the working lyrics;
- a `.json` file with metadata, source, offset, timestamps, and the preserved original.

A manual `.lrc` edit wins over the JSON copy. Metadata-only updates keep that manual text.
Files are written atomically. The legacy Python library is not read or migrated.

The data directory also contains `nico-karaoke.log` for windowless builds. Use
`NICO_KARAOKE_DATA` to point the app at another directory.

## Lyrics lookup

LRCLIB is queried without an API key. Titles and channel-style artist names are cleaned
first. The lookup tries an exact match with the album, then without it, then searches within
five seconds of the known duration. A synchronised result wins over plain text, and a miss is
cached for one hour. A temporary network failure is not cached as a miss.

## Checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm run typecheck --prefix web
npm run lint --prefix web
npm run format:check --prefix web
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
