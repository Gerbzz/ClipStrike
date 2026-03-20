# Media Readback Buffer Reuse Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add reusable readback helpers and thread them into the save-frame path so repeated exports do not allocate a fresh CPU buffer every time.

**Architecture:** Keep the reusable buffer in the UI layer where the save-frame action already lives, but move the row-copy logic into a render helper that can fill an existing buffer. Preserve the current owned-buffer API as a wrapper for existing call sites. The thumbnail fallback can adopt the same pattern later without changing the caller-facing behavior.

**Tech Stack:** Rust, wgpu, egui, image, tokio.

---

### Task 1: Add a reusable readback helper

**Files:**
- Modify: `crates/clipstrike-render/src/gpu.rs`
- Test: `crates/clipstrike-render/src/gpu.rs`

- [x] **Step 1: Write a failing unit test that proves a caller-owned buffer can be filled twice with different row layouts.**
- [x] **Step 2: Implement `read_texture_rgba_into` and keep the owned `read_texture_rgba` wrapper.**
- [x] **Step 3: Run the focused render test and `cargo check --all-targets`.**

### Task 2: Thread buffer reuse into save-frame export

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`
- Test: `crates/clipstrike-ui/src/ui.rs`

- [x] **Step 1: Add a failing test for the save-frame helper using a reusable buffer and a borrowed `save_buffer` call.**
- [x] **Step 2: Update `save_current_frame_to_path` to reuse the buffer and write through `image::save_buffer`.**
- [x] **Step 3: Run the focused UI test, the native smoke suite, and `cargo check --all-targets`.**


