# Media Hardening Plan

**Goal:** Close the remaining media-path gap by preserving hardware frame metadata, reducing CPU copies where possible, and keeping the GStreamer-to-wgpu path explicit and testable.

**Architecture:**
- Keep GStreamer frame availability and GPU frame ownership separated by a small metadata struct and tokio channels.
- Preserve external texture provenance in the texture pool so future zero-copy interop can be added without rewriting the UI path.
- Leave CPU readback only where it is explicitly needed for export or thumbnail fallbacks.

**Tech Stack:** Rust, GStreamer, wgpu, egui, tokio.

---

## Task 1: Preserve External Texture Metadata

**Files:**
- Modify: `crates/clipstrike-render/src/gpu.rs`
- Modify: `crates/clipstrike-media/src/engine.rs`
- Modify: `crates/clipstrike-ui/src/ui.rs`
- Test: `crates/clipstrike-render/src/gpu.rs`
- Test: `crates/clipstrike-ui/src/ui.rs`

- [x] **Step 1: Add a small metadata helper for frame upload requests and keep external texture ids on pooled frames.**
- [x] **Step 2: Drain pending upload requests before borrowing the texture pool in the UI loop.**
- [x] **Step 3: Verify the request/metadata path with unit tests.**

## Task 2: Reduce CPU Copies in Media Fallbacks

**Files:**
- Modify: `crates/clipstrike-media/src/probe.rs`
- Modify: `crates/clipstrike-render/src/gpu.rs`
- Modify: `crates/clipstrike-ui/src/ui.rs`
- Test: `crates/clipstrike-media/src/probe.rs`

- [x] **Step 1: Identify the remaining thumbnail/save-frame readback points.**
- [x] **Step 2: Replace avoidable per-row or per-frame CPU copies with the narrowest owned-buffer boundary that still satisfies the UI.**
- [x] **Step 3: Verify with focused tests and `cargo check --all-targets`.**

## Task 3: Evaluate External Texture Import

**Files:**
- Modify: `crates/clipstrike-render/src/gpu.rs`
- Modify: `crates/clipstrike-media/src/engine.rs`
- Test: `crates/clipstrike-render/src/gpu.rs`

- [x] **Step 1: Document the remaining interop constraints for direct external texture import.**
- [x] **Step 2: Add the smallest safe import hook or fallback point the backend can actually exercise.**
- [x] **Step 3: Verify the result on native desktop with `gstreamer-backend` enabled.**




