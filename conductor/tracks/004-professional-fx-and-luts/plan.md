# Plan: Professional FX & LUTs (Track 004)

## Phase 1: Grading & Keying (Mostly Complete)
- [x] **Task 1.1:** Implement Gamma correction shader in `fx.wgsl`.
- [x] **Task 1.2:** Implement Hue rotation (HSV) in `fx.wgsl`.
- [x] **Task 1.3:** Implement Chroma Key shader with threshold/fuzziness.
- [x] **Task 1.4:** Wire keying/grading parameters to the Properties panel.

## Phase 2: LUT Support
- [x] **Task 2.1:** Implement `.cube` file parser for 1D/3D LUTs.
- [x] **Task 2.2:** Setup 3D Texture support in `FxPipeline` for LUT mapping.
- [x] **Task 2.3:** Implement LUT application logic in `fx.wgsl`.

## Phase 3: Effects Browser & Interaction
- [x] **Task 3.1:** Build the "Effects" tab in the left panel.
- [x] **Task 3.2:** Implement dragging an effect from the library onto a timeline clip.
- [x] **Task 3.3:** Implement "Effect" stack on clips (allowing multiple shaders per clip).
    - [x] Add `effects` field to `Clip` struct and `Effect` model.
    - [x] Implement drag-and-drop to add effects to the clip stack.
    - [x] Update rendering pipeline to iterate over the effect stack (multipass support).

## Verification
- [ ] Run `cargo check --all-targets`.
- [ ] Manual test: Verify Chroma Key correctly removes green background.
- [ ] Manual test: Verify `.cube` LUT correctly changes color look.
