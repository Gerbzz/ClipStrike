# ClipStrike — FilmCraft editor source candidate

This is a source-only candidate to replace the old ClipStrike shell with the actual FilmCraft-based native editor. It retains FilmCraft's editor shell, engine, timeline, media pool, project model, codecs and export UI, with ClipStrike importer, branding, safety and playback changes. The separate Windows preview currently uses the `clipstrike-seek-next` binary; the primary local launcher has not been promoted.

The source is staged as a normal descendant of public `Gerbzz/Clipstrike` main at `fdf2f6ac8828374b803577ae1f6b310b73860075`. DemoStrike pins that historical commit. No repository history is rewritten, and this candidate has not been pushed.

## Reproduce the bounded Windows checkpoint

With Rust 1.95 or newer, from the repository root:

```powershell
$env:CARGO_BUILD_JOBS = '1'
$env:CARGO_INCREMENTAL = '0'
cargo test --locked -p filmcraft-ui-egui --lib -- --quiet
cargo test --locked -p filmcraft-ui-egui --test source_playback_ui -- --quiet
cargo test --locked -p filmcraft --bin clipstrike-seek-next --features seek-checkpoint -- --quiet
cargo build --locked -p filmcraft --bin clipstrike-seek-next --features seek-checkpoint
```

The original migration checkout passed 132 UI, 10 source-viewer and 48 app tests, built `clipstrike-seek-next.exe` at SHA-256 `80c85828719378c5bd7d1babd4933fddc9b86f43ef5f320b26b6289b7e1ec16d`, and passed a bounded real-project live-seek trace. Independent review reproduced importer refusals and seek/pause progression. This selected source package has a newly synthetic importer fixture and a reduced workspace lockfile, so **its own build/tests remain pending** while the heavy build slot is paused.

The `.cstrike` importer is conservative and does not modify input projects. Native mouse/keyboard and accessibility use, speaker output and end-to-end sound/picture sync remain unverified. The current preview checkpoint is not a release or general media-fidelity claim.

See [SOURCE_SCOPE.md](SOURCE_SCOPE.md) for exact source and licensing boundaries and [ATTRIBUTION.md](ATTRIBUTION.md) for bundled donor assets. No first-party ClipStrike license grant is made by the donor license files.
