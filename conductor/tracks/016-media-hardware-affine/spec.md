# Media Hardware-Affine Preview Track

**Goal:** Remove the remaining GStreamer-to-wgpu fallback boundary by negotiating GL-backed frames directly into the native preview sink when the backend supports it.

**Architecture:**
- Keep GStreamer negotiation inside `clipstrike-media`.
- Preserve the existing CPU fallback for environments that cannot negotiate GL-backed samples.
- Keep the UI consuming the same frame metadata shape so the render loop stays testable.

**Tech Stack:** Rust, GStreamer, gstreamer-gl, wgpu, egui, tokio.

---

## Task 1: Negotiate GL-backed Preview Samples

**Files:**
- Modify: `crates/clipstrike-media/src/engine.rs`
- Modify: `crates/clipstrike-render/src/gpu.rs`
- Test: `crates/clipstrike-media/src/engine.rs`

- [x] **Step 1: Add a unit-testable helper that advertises GLMemory caps for the preview sink when external texture import is preferred.**
- [x] **Step 2: Build the native video sink with a GL-backed path first, falling back to the existing CPU download path only when necessary.**
- [x] **Step 3: Verify the GStreamer backend tests and `cargo check --all-targets` stay green.**