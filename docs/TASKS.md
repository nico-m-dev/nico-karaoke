# Nico Karaoke — tasks

Current execution plan. Product direction lives in `docs/roadmap.md`; verified evidence lives
in `docs/testing.md`.

Status: rewrite delivered. Owner acceptance, the Windows SMTC run, and packaging remain.

## Owner acceptance (owner)

- [ ] Owner acceptance run in dev: use the app end to end locally and confirm packaging may
  start. This box belongs to the owner; agents wait until it is checked.
- [ ] Studio acceptance: transport while editing, play/pause state, timeline, tap mode,
  save/dirty handling, raw LRC, and the library round-trip.

## Windows verification

- [ ] Run the SMTC acceptance matrix on a real Windows machine with a media application
  before a Windows release.

## Packaging

- [ ] Switch the Tauri shell from the runtime `frontendDist` URL to embedded assets or a
  packaged resource. The executable-relative fallback is verified; a bundled resource path and
  a real package still need a build.
- [ ] Add Linux and Windows release builds to CI; WebKitGTK 4.1 and GTK 3 development packages
  are now installed on Linux runners. The icon resource is in place, so the Windows job gets
  past `tauri-build`; the Windows run itself still needs a real machine.
- [ ] Add semantic version tags, a changelog, and GitHub Releases with downloadable binaries.
- [ ] Choose and implement Linux bundle formats (`.deb`, AppImage, or both) and Windows code
  signing if distribution requires it.
- [ ] Optional: update notification against GitHub Releases, report-only.

## Notes

- The local Tauri shell, browser fallback, security headers, log rotation, and the current
  test suite are already in place.
- Remaining product follow-ups and later ideas are listed in `docs/roadmap.md`; add them here
  only when they become active work.
