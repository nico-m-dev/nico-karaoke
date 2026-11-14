# Changelog

All notable changes to Nico Karaoke are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Create lyrics when a track's lookup found nothing: the Library now offers **New lyrics** and
  a card for the current track, opening an unsaved Studio document bound to that track.
- Recognise a credit inside a video title such as `Artist - Song` or
  `Artist - Song (Official Video)` when matching a playing track to saved lyrics, so one song
  keeps one library record across YouTube and YouTube Music. The library keeps one record per
  artist and title; existing duplicates are not merged.
- Retry the lyrics lookup with the performer read from the title when the normal query finds
  nothing, so videos from label or re-upload channels such as Out Of Line Music or Sindhu
  World find their LRCLIB entry.

## [1.0.0] - 2026-11-13

First public release of the Rust and Tauri rewrite.

### Added

- Synchronised lyrics for the song playing in another application, with the active line
  filling as playback advances and wrapped lines filling in visual reading order.
- MPRIS support on Linux and SMTC support on Windows, including player discovery, selection,
  play, pause, and seek where the player supports it.
- LRCLIB lookup with a local search ladder, plus a searchable library of saved `.lrc` and
  `.json` records.
- A Studio for correcting text and timing, with tap mode, raw LRC editing, nudging, and
  restoring the saved original.
- Five accent palettes with matching vintage instrument artwork on the idle screen. The choice
  is remembered and applies to the player, library, and Studio.
- English and Dutch interface, with a persisted language choice.
- Localhost-only server with `Host` and `Origin` validation, a self-only content security
  policy, atomic library writes, and no telemetry or audio capture.
- Portable, self-contained desktop builds for Linux x86_64 and Windows x86_64. The frontend is
  embedded in the executable; no separate web folder is required.

### Known limitations

- The Windows executable is not code-signed, so SmartScreen may show a warning.
- The builds are portable archives, not installers, and there is no automatic update.
- The Linux build is made on Ubuntu 24.04 and needs WebKitGTK 4.1, GTK 3, libsoup 3, and
  librsvg at runtime.
- Only x86_64 Linux and Windows are built. macOS is not released.

[1.0.0]: https://github.com/nico-m-dev/nico-karaoke/releases/tag/v1.0.0
