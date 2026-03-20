# Plan: External App Bridges (Track 006)

## Phase 1: Configuration & UI Shell
- [x] **Task 1.1:** Add `blender_path` and `inkscape_path` fields to `AppConfig` and the Preferences UI.
- [x] **Task 1.2:** Add an "Advanced Titles" UI panel listing 3D Blender templates.
- [x] **Task 1.3:** Implement a status check mechanism to verify external executables exist before allowing integration.

## Phase 2: Blender Bridge (3D Titles)
- [x] **Task 2.1:** Create `crates/clipstrike-core/src/bridges/blender.rs` with logic to spawn `blender.exe` using `std::process::Command`.
- [x] **Task 2.2:** Write a Python template script that Blender can run headlessly to modify text objects and render an image sequence.
- [x] **Task 2.3:** Implement async rendering progress tracking and automatic import of the resulting image sequence into the timeline.

## Phase 3: Inkscape Bridge (Vector Titles)
- [x] **Task 3.1:** Create `crates/clipstrike-core/src/bridges/inkscape.rs` to spawn `inkscape.exe`.
- [x] **Task 3.2:** Implement an "Edit in Inkscape" context menu option on SVG assets.
- [x] **Task 3.3:** Use `notify` crate to watch the edited SVG file for saves and trigger a texture reload in the `wgpu` backend.

## Verification
- [ ] Run `cargo check --all-targets`.
- [ ] Manual test: Generate a 3D title using a mock Blender python script.
- [ ] Manual test: Edit an SVG in Inkscape and verify the texture updates in the Media Browser/Timeline instantly.
