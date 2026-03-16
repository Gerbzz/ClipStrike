# Phase 16: Advanced Export Engine Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a professional export system with support for multiple formats (MP4, WebM, MOV) and hardware-accelerated encoding.

**Architecture:**
1.  **Define Export Models**: Create `ExportFormat` and `VideoCodec` enums.
2.  **Dynamic Pipeline Construction**: Refactor `Exporter` to link different encoders/muxers based on settings.
3.  **Hardware Encoding Affinity**: Prefer `nvh264enc`, `d3d11h264enc`, or `qsvh264enc` if available.
4.  **UI Dialog**: Build a professional export settings dialog in egui.

**Tech Stack:** Rust, GStreamer, egui.

---

## Chunk 1: Export Models & Infrastructure

### Task 1: Define Export Settings

**Files:**
- Modify: `crates/clipstrike-media/src/export.rs`

- [ ] **Step 1: Define `ExportFormat` and `VideoCodec` enums.**

```rust
// In crates/clipstrike-media/src/export.rs
pub enum ExportFormat {
    MP4,
    WebM,
    MOV,
    AVI,
}

pub enum VideoCodec {
    H264,
    H265,
    VP9,
    ProRes,
}
```

- [ ] **Step 2: Add `ExportSettings` struct to encapsulate bitrate, resolution, and format.**

- [ ] **Step 3: Verify compilation.**

Run: `cargo check -p clipstrike-media`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-media/src/export.rs
git commit -m "feat: define advanced export models and settings"
```

---

## Chunk 2: Dynamic Pipeline Refactoring

### Task 2: Implement Dynamic Pipeline in Exporter

**Files:**
- Modify: `crates/clipstrike-media/src/export.rs`

- [ ] **Step 1: Refactor `Exporter::new` to use match statements for selecting GStreamer elements.**

- [ ] **Step 2: Add logic to prefer hardware-accelerated encoders.**

- [ ] **Step 3: Update `push_frame` to handle timestamping correctly for variable frame rates.**

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-media/src/export.rs
git commit -m "feat: implement dynamic GStreamer pipeline for export"
```

---

## Chunk 3: UI Export Dialog

### Task 3: Build the Export Dialog

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`

- [ ] **Step 1: Create a modal window for "Export Video".**

- [ ] **Step 2: Add selection for Format, Codec, and Resolution.**

- [ ] **Step 3: Hook up the "Export" button to start the background render task.**

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-ui/src/ui.rs
git commit -m "feat: build professional export dialog in UI"
```
