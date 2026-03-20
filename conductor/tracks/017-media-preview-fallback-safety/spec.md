# Media Preview Fallback Safety Track

**Goal:** Keep the native preview path functional when GStreamer produces GL-backed samples by preserving the CPU upload fallback while still retaining external texture metadata for future import work.

**Architecture:**
- Keep sample inspection in `clipstrike-media`.
- Keep `external_texture_id` as additive metadata, not a hard switch that suppresses fallback pixels.
- Preserve the UI/render contract so the native desktop app still shows a real frame while the backend evolves.

**Tech Stack:** Rust, GStreamer, gstreamer-gl, wgpu, egui, tokio.

---

## Task 1: Preserve CPU Fallback for GL-backed Samples

**Files:**
- Modify: `crates/clipstrike-media/src/engine.rs`
- Test: `crates/clipstrike-media/src/engine.rs`

- [x] **Step 1: Add a failing test that a GL-backed frame request still preserves the pixel payload when the sample is readable.**
- [x] **Step 2: Refactor request construction so external texture metadata and CPU fallback pixels can coexist.**
- [x] **Step 3: Verify the focused GStreamer test and `cargo check --all-targets`.**
