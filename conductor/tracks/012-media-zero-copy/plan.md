# Media Zero-Copy Track

**Goal:** Finish the remaining media-path work by turning GStreamer GL-backed samples into explicit texture metadata and preparing the render path for eventual zero-copy interop.

**Architecture:**
- Keep the GStreamer sample inspection in `clipstrike-media` so the UI never touches GStreamer objects directly.
- Preserve external texture provenance in `clipstrike-render` and keep CPU readback as a fallback path only.
- Keep all async communication on `tokio` channels and avoid adding shared locks while the media path is being refined.

**Tech Stack:** Rust, GStreamer, gstreamer-gl, wgpu, egui, tokio.

---

## Task 1: Detect GL-backed Samples

**Files:**
- Modify: `crates/clipstrike-media/src/engine.rs`
- Test: `crates/clipstrike-media/src/engine.rs`

- [x] **Step 1: Add a helper that detects `GLMemory` on a sample and preserves the texture id.**
- [x] **Step 2: Keep the CPU fallback path for samples that are not GL-backed.**
- [x] **Step 3: Verify the helper with focused unit tests and the GStreamer backend env.**

## Task 2: Thread Texture Metadata Into Preview Selection

**Files:**
- Modify: `crates/clipstrike-render/src/gpu.rs`
- Modify: `crates/clipstrike-ui/src/ui.rs`
- Test: `crates/clipstrike-render/src/gpu.rs`
- Test: `crates/clipstrike-ui/src/ui.rs`

- [x] **Step 1: Decide how the preview path should recognize external texture metadata without changing the UI ownership model.**
- [x] **Step 2: Add the smallest render-path hook that can exercise the metadata without introducing a CPU readback regression.**
- [x] **Step 3: Verify the preview path still passes `cargo check --all-targets` and the desktop smoke tests.**

## Task 3: Hardware-Affine Verification

**Files:**
- Modify: `crates/clipstrike-media/src/engine.rs`
- Modify: `conductor/tracks.md`
- Test: `crates/clipstrike-ui/tests/native_smoke.rs`

- [x] **Step 1: Add a native smoke check that proves the GStreamer backend still launches with the documented SDK env.**
- [x] **Step 2: Capture the remaining hardware-import constraints in the implementation state doc.**
- [x] **Step 3: Re-run the native desktop suite and `cargo check --all-targets`.**
