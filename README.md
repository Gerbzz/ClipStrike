# ClipStrike

**by PWDS**

**Your footage. Your timeline.**

ClipStrike is a desktop workspace for turning source footage into an edit. Organize media, review clips, build a cut across tracks, and save an editable project; its `.cstrike` importer reads capture packages without changing the originals.

## Highlights

- Arrange video and audio on a multitrack timeline.
- Keep source media and project edits organized.
- Bring supported `.cstrike` capture packages into the editor.

## Current status

Source-only Windows editor candidate. The separate `clipstrike-seek-next` preview has had bounded local checks, but this published source package has not completed its own build and test pass. Native interaction, speaker output, and end-to-end sound/picture sync remain unverified; no release installer is provided.

## Edit a project

1. Bring source footage into the media pool and review it in the source viewer.
2. Place the parts you want on video and audio tracks, then save the editable project.
3. For a supported capture handoff, use the `.cstrike` importer. It reads the input conservatively and leaves the source project unchanged.

The current source checkout still needs its own build and hands-on playback review before these steps can serve as release instructions.

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

The original migration checkout passed 132 UI, 10 source-viewer and 48 app tests, built `clipstrike-seek-next.exe` at SHA-256 `80c85828719378c5bd7d1babd4933fddc9b86f43ef5f320b26b6289b7e1ec16d`, and passed a bounded real-project live-seek trace. Independent review reproduced importer refusals and seek/pause progression. This selected source package has a newly synthetic importer fixture and a reduced workspace lockfile, so **its own build/tests remain pending**.

Native mouse/keyboard and accessibility use, speaker output and end-to-end sound/picture sync remain unverified. The current preview checkpoint is not a release or general media-fidelity claim.

See [SOURCE_SCOPE.md](SOURCE_SCOPE.md) for exact source and licensing boundaries and [ATTRIBUTION.md](ATTRIBUTION.md) for bundled donor assets. No first-party ClipStrike license grant is made by the donor license files.

## Next steps

Build and test this exact source package, then verify native controls, speaker output, and sound/picture sync with reviewed footage before packaging a release.

## Source history and credits

This editor candidate retains FilmCraft's editor, engine, timeline, media pool, project model, codec, and export UI, with ClipStrike importer, branding, safety, and playback changes. It descends from the earlier ClipStrike main at `fdf2f6ac8828374b803577ae1f6b310b73860075`, which DemoStrike still pins. See [SOURCE_SCOPE.md](SOURCE_SCOPE.md) for the exact source and license boundary and [ATTRIBUTION.md](ATTRIBUTION.md) for bundled donor assets. Donor license files do not grant terms for all first-party ClipStrike changes.
