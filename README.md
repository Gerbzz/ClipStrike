# ClipStrike

**A native Rust editing workbench for assembling CS2 footage.** ClipStrike brings media organization, a multitrack timeline, clip properties, and capture-package handoff into one desktop project. It is an active prototype for creators who want to turn recorded takes into an editable cut, with the project and media pipeline kept in separate crates.

> **Development status:** The editor model, desktop UI, project save/recovery, and capture-package import have automated coverage. The GStreamer media backend is optional and disabled in the default build. Real playback, audible audio, and encoded export have not passed production acceptance.

## The editing workflow

1. Create a project and organize source clips in the media browser.
2. Arrange clips on video and audio tracks; use trim, slice, snapping, markers, and undo/redo while building a cut.
3. Adjust clip properties through the timeline and keyframe model. The repository contains transitions, effects, titles, waveform helpers, and export controls, but their presence does not establish end-to-end output quality.
4. Save a native project and keep its source media available. If a source moves or its hash changes, the capture-package path reports it as offline instead of silently dropping timeline edits.

A capture-package importer is also included for externally prepared takes. It creates separate editable assets and timeline clips from a versioned package; it does not record gameplay itself.

## What is in this repository

| Area | Implemented surface | Current boundary |
| --- | --- | --- |
| Timeline and project | Multitrack clip model, editing commands, snapping, markers, keyframes, transitions, save/reopen, and recovery paths | Frame-exact editing at fractional frame rates is not established |
| Desktop UI | First-party Rust application built with egui/eframe and wgpu-backed rendering components | Visible native-window review is still pending |
| Media | GStreamer-based probing, playback, and export code behind a Cargo feature | Default builds disable the backend; real playback and encoded export with sound are unverified |
| Capture handoff | Package validation, import into a new project, provenance, offline detection, and relinking | Package metadata and stream probes do not prove audible sound or capture origin |
| Development tests | Core, UI harness, and synthetic package tests | Native media smoke tests need a configured desktop/media environment; one import UI harness is ignored by default |

The code is organized as six Cargo crates: <code>clipstrike-core</code> owns project and timeline rules; <code>clipstrike-ui</code> owns the editor interface; <code>clipstrike-media</code> owns media operations; <code>clipstrike-render</code> owns GPU composition helpers; <code>clipstrike-audio</code> owns audio helpers; and <code>clipstrike-app</code> launches the desktop window. See [architecture](docs/ARCHITECTURE.md) for the deeper module layout.

## Editor concepts in this prototype

**The timeline is the source of truth.** A project stores tracks, clips, assets, edits, properties, and capture provenance. Core commands cover selecting, placing, trimming, slicing, copying, pasting, and undoing edits. Snapping and markers support alignment. The UI also has media-browser and thumbnail-oriented flows for locating material before it reaches the timeline.

**Properties can change over time.** The code includes clip transforms, keyframes with multiple interpolation modes, transition configuration, effect parameters, title editing, and waveform helpers. These are editor and rendering foundations. Their existence in the source is not a claim that every combination has been visually reviewed or exported correctly.

**Recovery keeps the edit intact.** Project writes use a temporary file and a previous-version backup; autosave and recovery checkpoints are represented in the app. Capture imports retain media hashes and path hints. If linked media is missing, moved, or changed, the editor can mark it offline and leave the project structure available for relinking.

**Media is a separate integration layer.** The wgpu renderer and GStreamer integration live outside the timeline model. That separation lets core editing behavior be tested with synthetic assets, while actual playback, audio, and encoded output can be validated against real footage later. The optional GStreamer SDK is a native dependency, not bundled project code.

## Build and run

The published checkout is developed and verified on Windows with a Rust toolchain that supports edition 2024. Cargo resolves the pinned versions in <code>Cargo.lock</code>.

~~~powershell
cargo test --locked --workspace
cargo build --locked --workspace --bins
cargo run --locked -p clipstrike-app
~~~

This default launch is useful for inspecting the application and editor state. It does **not** enable real GStreamer playback or export.

To work on the native media path, install a compatible GStreamer MSVC SDK yourself and use the repository launcher:

~~~powershell
.\run_clipstrike_gstreamer.cmd
~~~

The script looks for a repository-local SDK folder and then the standard Windows Program Files SDK location. The SDK is not included here, and running this command is not an acceptance test for finished playback or export. [Local setup](docs/LOCAL_SETUP.md) lists the environment variables for manual backend checks. Native libraries are an intentional part of this Rust application; it is not described as a pure-Rust codec implementation.

## Capture packages and project files

A compatible capture package contains a versioned <code>manifest.json</code> and individual media files under <code>clips/</code>. The importer validates package structure and hashes, then writes a new <code>.cstrike</code> project:

~~~powershell
cargo run --locked -p clipstrike-core --bin clipstrike-import -- <package-folder> <new-project.cstrike>
~~~

The project links to the package media rather than embedding it. Keep the package beside the project when moving work between folders. The project loader can report missing or changed sources and preserve edits for relinking. An interrupted project save retains a backup for recovery. Integer FPS is a temporary import gate; the millisecond timeline does not yet guarantee every frame boundary, even at 60 fps.

Importing a package is a handoff, not a quality verdict. A package marked “stream probed” has reported stream headers; ClipStrike has not independently established audible sound, footage origin, or final editorial quality.

## Current limits and direction

ClipStrike is a source build and local prototype. There is no published installer or release in this repository. Clean-machine setup, visible desktop review, real media playback, export with audio, power-loss recovery, and broader format coverage still need acceptance work.

Planned work includes validating the full media path on actual captures, tightening frame timing and relinking behavior, improving editor interaction, and testing packaging on clean Windows machines. These are goals, not shipped features.

## Development and contribution

Start changes in the crate that owns the behavior: timeline and project rules in <code>clipstrike-core</code>, widgets and interaction in <code>clipstrike-ui</code>, decoding/probing/export in <code>clipstrike-media</code>, GPU composition in <code>clipstrike-render</code>, and audio helpers in <code>clipstrike-audio</code>. Keep the application crate thin. Add focused tests for core rules and use the UI harness for interactions that can run without external media.

The [current state](docs/CLIPSTRIKE_STATE.md), [architecture](docs/ARCHITECTURE.md), and [local setup](docs/LOCAL_SETUP.md) documents contain implementation context. Media-related changes need separate real-file checks in a configured environment; synthetic fixture tests alone cannot establish playback or audible sound.

## License

ClipStrike source is licensed under [MIT](LICENSE). Third-party native libraries and Rust crates retain their own licenses.
