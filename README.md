# Nico Karaoke

Synchronised lyrics for whatever is playing in another application. The app reads the
current track and position from MPRIS on Linux or SMTC on Windows, fetches lyrics from
LRCLIB, and draws a line-by-line karaoke view in a local web interface.

This repository is a clean Rust and TypeScript rewrite of an earlier Python application. It
does not record audio, download tracks, or play video.

## Status

Development phase. The workspace, local server, typed frontend, media providers, lyrics
lookup, library, watcher, local API, player UI, and library browser are implemented; the
Studio timeline editor comes next. The server binds to `127.0.0.1` by default and validates
`Host`/`Origin`.

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

## Media providers

Linux follows players through MPRIS on the session bus. Windows follows sessions through
SMTC. Both expose the same provider trait and never capture audio.

With a player active, the developer probe prints what the backend sees:

```bash
cargo run -p nico-karaoke-platform --example media_probe
```

## Player

The player follows the current track and keeps the active lyric on the centre line. The
active line fills green until the next line or a `♪` break. Upcoming lines are lighter and
past lines fade; unsynchronised lyrics fall back to a readable scrollable list.

| Input | Action |
| --- | --- |
| `Space` | play / pause the followed player |
| `F` | toggle fullscreen |
| player picker | choose which player to follow |
| `Library` | open the searchable library browser |
| `NL` / `EN` | switch language from the idle screen |

The bars fade after a few seconds while playing; move the mouse or press a key to bring them
back.

## Library

The library opens from the player and lists every saved text. Search filters by title,
artist, or album. Each item shows its source, sync state, line count, update date, current
badge, and whether an original is kept.

Open an item to edit the raw LRC, save it, restore the saved original, delete it, or search
for it on YouTube or Spotify. The Studio timeline editor is the next phase.

## Local API

The local HTTP API returns JSON and never caches responses. It is only reachable from this
machine and rejects foreign `Host`/`Origin` headers.

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/health` | server and platform status |
| `GET` | `/api/now` | current track, players, and lyrics state |
| `POST` | `/api/player` | choose the followed player |
| `POST` | `/api/control` | play, pause, or seek |
| `GET` | `/api/library` | library summaries |
| `POST` | `/api/library` | create a library item |
| `GET` | `/api/library/{key}` | full library item |
| `PUT` | `/api/library/{key}` | save edited text |
| `DELETE` | `/api/library/{key}` | delete an item |
| `POST` | `/api/library/{key}/original` | restore the saved original; send `{}` as JSON |
| `POST` | `/api/lyrics` | save text for the current track |
| `POST` | `/api/lyrics/offset` | update the current offset |
| `POST` | `/api/lyrics/fetch` | look up lyrics without saving |

Errors use stable codes such as `forbidden_host`, `no_current_track`, `track_changed`,
`lyrics_not_found`, `invalid_json`, and `unsupported_media_type`.

`signature` identifies the current track by normalised artist and title; `record_key`
identifies the library file. `item_key` in lookup requests is a library key.

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
