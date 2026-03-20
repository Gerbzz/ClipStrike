# Media Preview Fast Path

**Goal:** Allow the preview path to skip the GPU FX pass when a frame is external-backed and the clip has no active visual effects. This keeps the external-texture metadata actionable without introducing a CPU readback regression.

**Architecture:**
- Keep the decision logic in `clipstrike-ui` so the render loop can choose between raw texture presentation and the FX pipeline without exposing GStreamer details to the UI.
- Reuse the existing `FrameSourceKind` metadata from `clipstrike-render`.
- Keep the fallback path intact for clips that do have active visual effects.

**Tech Stack:** Rust, egui, wgpu, tokio.

---

## Chunk 1: Neutral External-Frame Bypass

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`
- Test: `crates/clipstrike-ui/src/ui.rs`

- [x] **Step 1: Write a failing unit test for the preview bypass decision when an external-backed frame has neutral clip effects.**
- [x] **Step 2: Implement the smallest helper that decides when the preview can skip the FX pass.**
- [x] **Step 3: Re-run `cargo check --all-targets` and the focused UI tests.**

