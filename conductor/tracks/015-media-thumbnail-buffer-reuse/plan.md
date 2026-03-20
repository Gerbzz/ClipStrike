# Media Thumbnail Buffer Reuse Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reusable thumbnail readback helper and keep the current owned thumbnail API as a wrapper for existing callers.

**Architecture:** Keep the readback copy logic near `clipstrike-media::probe`, since that module already owns the GStreamer thumbnail pipeline. Add a buffer-filling helper so callers can reuse an allocation across thumbnail generations. Preserve the current `Vec<u8>`-returning API for compatibility, but have it delegate to the reusable helper.

**Tech Stack:** Rust, GStreamer, wgpu, tokio.

---

### Task 1: Add a reusable thumbnail readback helper

**Files:**
- Modify: `crates/clipstrike-media/src/probe.rs`
- Test: `crates/clipstrike-media/src/probe.rs`

- [x] **Step 1: Write a failing unit test that proves a caller-owned thumbnail buffer can be filled twice without changing pointers.**
- [x] **Step 2: Implement the buffer-filling helper and keep the owned thumbnail wrapper.**
- [x] **Step 3: Run the focused media test and `cargo check --all-targets`.**


