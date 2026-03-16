# ClipStrike

ClipStrike is a Rust rewrite of the OpenShot editing stack, organized as a Cargo workspace.

## Workspace

- `crates/clipstrike-core`: timeline/editor engine
- `crates/clipstrike-audio`: waveform extraction and audio-focused helpers
- `crates/clipstrike-media`: GStreamer probing, decode/playback, export, proxy adapters
- `crates/clipstrike-render`: `wgpu` textures, preview readback, FX pipeline
- `crates/clipstrike-ui`: `egui` editor frontend and interaction controllers
- `crates/clipstrike-app`: desktop binary wiring the workspace together

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the current layout and contributor guidance.

## Common Commands

- `cargo test -p clipstrike-core`
- `cargo check -p clipstrike-ui`
- `cargo check -p clipstrike-app`
- `cargo check -p clipstrike-app --features gstreamer-backend`
- `run_clipstrike_gstreamer.cmd`
