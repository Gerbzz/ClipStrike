# ClipStrike Media Preview Fallback Safety Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep preview correct when GStreamer produces GL-backed samples by preserving the existing CPU upload fallback while still retaining external texture metadata for future import work.

**Architecture:**
- Keep sample inspection in `clipstrike-media` and metadata propagation in `clipstrike-render`.
- Treat `external_texture_id` as additive metadata for now so the current preview path remains functional.
- Keep the UI unchanged except for the smallest testable helper that resolves request construction.

**Tech Stack:** Rust, GStreamer, gstreamer-gl, wgpu, egui, tokio.

---

### Task 1: Preserve CPU Fallback for GL-backed Samples

**Files:**
- Modify: `crates/clipstrike-media/src/engine.rs`
- Test: `crates/clipstrike-media/src/engine.rs`

- [ ] **Step 1: Add a failing test that a GL-backed frame request still preserves the pixel payload when the sample is readable.**
- [ ] **Step 2: Refactor request construction so external texture metadata and CPU fallback pixels can coexist.**
- [ ] **Step 3: Re-run the focused GStreamer test and `cargo check --all-targets`.**
