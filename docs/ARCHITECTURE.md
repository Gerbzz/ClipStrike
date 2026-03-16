# ClipStrike Architecture

ClipStrike is organized as a Cargo workspace so the Rust rewrite can mirror OpenShot's product boundaries without scattering logic across unrelated files.

## Workspace Layout

- `crates/clipstrike-core`
  - Rust equivalent of the engine-facing parts of `libopenshot`
  - timeline model
  - project model and serialization
  - snapping, transitions, keyframes, undoable commands
  - editor/session tests and pure behavior tests
- `crates/clipstrike-audio`
  - waveform extraction and audio-specific helpers
  - isolated from UI so import/playback audio paths stay testable
- `crates/clipstrike-media`
  - GStreamer probing, playback engines, export, frame provider, proxy generation
  - owns backend integration instead of editor behavior
- `crates/clipstrike-render`
  - `wgpu` texture pool, frame readback, compute-FX pipeline, shaders
- `crates/clipstrike-ui`
  - editor frontend
  - `egui` views, app config, and interaction controllers
  - depends on `clipstrike-core` instead of owning timeline logic directly
- `crates/clipstrike-app`
  - desktop binary wiring the workspace crates together

## Design Rules

- Timeline rules belong in `clipstrike-core`, not in the UI crate.
- UI gesture handlers should call into tested core behavior, then render the result.
- GStreamer stays in `clipstrike-media`; GPU code stays in `clipstrike-render`.
- `clipstrike-app` should remain a thin startup/wiring layer.
- New contributors should add model tests to `clipstrike-core` before wiring new editor gestures.

## Commands

- Core tests: `cargo test -p clipstrike-core`
- UI compile check: `cargo check -p clipstrike-ui`
- App compile check: `cargo check -p clipstrike-app`
- GStreamer-backed app check: `cargo check -p clipstrike-app --features gstreamer-backend`
- Run the app with GStreamer on Windows: `run_clipstrike_gstreamer.cmd`

## Current Boundary

The current split is now aligned with the target workspace layout:
- `clipstrike-core` is the engine-facing rewrite target.
- `clipstrike-ui` is the OpenShot-Qt-equivalent frontend layer.
- `clipstrike-media` and `clipstrike-render` hold backend/platform concerns.
- `clipstrike-app` should stay intentionally small.

The next cleanup target is to keep extracting timeline interaction state from `clipstrike-ui` into `clipstrike-core` so drag, trim, snap, and selection behavior are driven by the engine layer.
