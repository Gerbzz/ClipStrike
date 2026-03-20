# Plan: Advanced Compositing (Track 008)

## Phase 1: Solid Color Generators
- [x] **Task 1.1:** Define `GeneratorAsset` and `GeneratorType` in `crates/clipstrike-core/src/project.rs`.
- [x] **Task 1.2:** Implement `GeneratorPipeline` in `crates/clipstrike-render/src/lib.rs` using a compute shader.
- [x] **Task 1.3:** Add "Add Generator" to the Media Browser UI.

## Phase 2: On-Canvas Transform Handles
- [x] **Task 2.1:** Implement coordinate translation between egui screen space and project coordinate space.
- [x] **Task 2.2:** Draw draggable handles (rectangles) over the selected clip in the video preview.
- [x] **Task 2.3:** Update clip properties (Pos, Scale) in real-time as handles are dragged.

## Phase 3: Global Watermarks
- [x] **Task 3.1:** Add `watermark_asset_id` and watermark properties to `Project` struct.
- [x] **Task 3.2:** Update `CompositorPipeline` to apply the watermark as a final pass.
- [x] **Task 3.3:** Add a "Set as Watermark" option to the Media Browser context menu.

## Verification
- [ ] Run `cargo check --all-targets`.
- [ ] Manual test: Add a red solid color generator to the timeline.
- [ ] Manual test: Resize and move a clip using on-canvas handles.
- [ ] Manual test: Set a logo PNG as a watermark and verify it stays on top during playback.
