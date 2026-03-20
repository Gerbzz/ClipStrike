# ClipStrike

ClipStrike is a Rust-based desktop video editor focused on native performance, clear subsystem boundaries, and contributor-friendly architecture. The project is organized as a multi-crate Cargo workspace with a hardware-accelerated UI stack built on `egui` and `wgpu`, plus a GStreamer-backed media pipeline for playback, probing, and export.

The goal is straightforward: build a serious non-linear editor with a clean Rust architecture, practical test coverage, and a development workflow that makes it easier for contributors to improve real editing features without fighting a monolith.

## Why ClipStrike

- Native desktop editor architecture in Rust
- Clear crate boundaries between core logic, media, render, audio, UI, and app wiring
- Hardware-accelerated rendering path with `wgpu`
- GStreamer-backed media pipeline for playback and export workflows
- Contributor-oriented codebase with tests around timeline behavior, editor actions, and desktop smoke flows

## Current Status

ClipStrike is in active development and already contains a substantial editor foundation. The current workspace includes timeline editing, playback controls, export plumbing, transitions, effects, title editing, markers, autosave and recovery, thumbnail flows, and early AI/vision tooling.

The project is usable as a development build, but it is still being hardened. The biggest currently documented rendering boundary is native zero-copy preview interop: the media pipeline can preserve external texture metadata, but the present `wgpu` stack still requires a renderer-owned texture for visible desktop preview frames, so some paths still fall back to CPU readback.

If you want to contribute, this is the right stage to get involved: the architecture is established, major editor systems exist, and there is still meaningful room to improve media handling, performance, UX polish, testing, and feature depth.

## Highlights

- Multi-track timeline editing with drag, trim, slice, copy/paste, snapping, and undo/redo
- Playback controls with seek, stepping, and timeline interaction coverage
- Keyframe-driven clip properties with linear, bezier, and constant interpolation
- Transition support with overlap-driven crossfades and transition configuration
- GPU-backed effects pipeline with blur, opacity, transform, brightness, and blend modes
- Media browser with import, filtering, and thumbnail-oriented workflows
- SVG title tooling and built-in title editing support
- Marker editing directly on the timeline ruler
- Export models, export settings, progress reporting, cancel flow, and preset-driven pipeline work
- Autosave and recovery checkpoint support
- Native smoke tests and in-process harnesses for desktop editor flows
- Early AI/vision features including detection labels and face-blur-oriented groundwork

## Workspace Layout

The repository is split into focused crates so behavior can live in the right layer:

- `crates/clipstrike-core`
  Timeline model, project model, snapping, keyframes, transitions, undoable commands, and core editor behavior.
- `crates/clipstrike-audio`
  Waveform extraction and audio-focused helpers.
- `crates/clipstrike-media`
  GStreamer probing, playback engines, export, frame provisioning, and proxy-related media tasks.
- `crates/clipstrike-render`
  `wgpu` texture management, frame readback, shader-driven effects, and render pipeline helpers.
- `crates/clipstrike-ui`
  `egui` editor frontend, app state, layout, and interaction controllers.
- `crates/clipstrike-app`
  Thin desktop binary layer that wires the workspace together.

More detail is available in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Getting Started

ClipStrike is currently developed and verified primarily on Windows.

### Prerequisites

- Rust toolchain with Cargo
- Windows development environment
- GStreamer MSVC SDK for the native media backend

The repo currently documents two known GStreamer SDK roots:

- `C:\Program Files\gstreamer\1.0\msvc_x86_64`
- `W:\Youtube\clipstrike\gst_extracted\PFiles64\gstreamer\1.0\msvc_x86_64`

### Quick Commands

```powershell
cargo check --all-targets
cargo test -p clipstrike-core
cargo test -p clipstrike-ui --lib
cargo check -p clipstrike-app
cargo check -p clipstrike-app --features gstreamer-backend
```

To launch with the repo's Windows GStreamer setup:

```powershell
.\run_clipstrike_gstreamer.cmd
```

### GStreamer Environment

When running GStreamer-backed commands in a fresh shell, set the SDK root and related environment variables:

```powershell
$env:GST_ROOT = 'C:\Program Files\gstreamer\1.0\msvc_x86_64'
$env:PATH = "$env:GST_ROOT\bin;$env:PATH"
$env:GST_PLUGIN_SYSTEM_PATH = "$env:GST_ROOT\lib\gstreamer-1.0"
$env:GST_PLUGIN_PATH = "$env:GST_ROOT\lib\gstreamer-1.0"
$env:PKG_CONFIG = "$env:GST_ROOT\bin\pkg-config.exe"
$env:PKG_CONFIG_PATH = "$env:GST_ROOT\lib\pkgconfig;$env:GST_ROOT\lib\gstreamer-1.0\pkgconfig"
$env:GSTREAMER_1_0_ROOT_MSVC_X86_64 = $env:GST_ROOT
```

Sanity checks:

```powershell
& "$env:PKG_CONFIG" --modversion glib-2.0
& "$env:PKG_CONFIG" --modversion gstreamer-1.0
& "$env:GST_ROOT\bin\gst-inspect-1.0.exe" --version
```

More setup detail lives in [docs/LOCAL_SETUP.md](docs/LOCAL_SETUP.md).

## Development Workflow

ClipStrike is intentionally structured so contributors can work on editor behavior without crossing too many layers:

- Put timeline rules and editing behavior in `clipstrike-core`
- Keep GStreamer and media backend work in `clipstrike-media`
- Keep GPU/rendering concerns in `clipstrike-render`
- Use `clipstrike-ui` for presentation and interaction wiring, not core timeline ownership
- Keep `clipstrike-app` thin

If you are adding or changing behavior, prefer adding tests in the crate that owns that behavior before wiring the UI around it. The codebase already includes unit tests, harness-driven UI tests, and native smoke coverage, so new work should extend that pattern rather than bypass it.

## Contributing

Contributions are welcome, especially in these areas:

- media pipeline stability and playback correctness
- performance and allocation cleanup
- editor UX polish and workflow refinement
- export reliability and preset coverage
- testing depth for native desktop interactions
- roadmap feature work in focused, reviewable slices

Before opening a larger change:

- Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Review [docs/CLIPSTRIKE_STATE.md](docs/CLIPSTRIKE_STATE.md) for the current implemented surface
- Check [TODO.md](TODO.md) and [BLOCKER.md](BLOCKER.md) for active priorities and known boundaries

Good contribution guidelines for this repo:

- keep changes focused
- preserve crate boundaries
- add or update tests with behavior changes
- avoid unrelated refactors in feature branches
- document new setup requirements and verification steps

## Near-Term Focus

Current high-value areas for contribution include:

- improving preview and playback performance on native desktop builds
- reducing remaining CPU readback paths in preview-related flows
- extending export presets and backend robustness
- deepening AI/vision tooling where the current groundwork already exists
- continuing UI polish, recovery flow refinement, and editing ergonomics
- expanding contributor documentation and reproducible verification paths

## Additional Docs

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/LOCAL_SETUP.md](docs/LOCAL_SETUP.md)
- [docs/CLIPSTRIKE_STATE.md](docs/CLIPSTRIKE_STATE.md)
- [TODO.md](TODO.md)
- [BLOCKER.md](BLOCKER.md)

## Roadmap Note

Screenshots and polished visual assets are intentionally not included yet. The README is currently optimized to give contributors a clear, professional entry point into the project while the editor UI continues to mature.
