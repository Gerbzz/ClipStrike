# UI/UX Polish Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add session recovery checkpoints for the native desktop app so unsaved work can be restored after a crash or forced exit.

**Architecture:** Reuse the existing app config autosave settings, derive recovery checkpoint paths from the current project location when available, and keep the actual checkpoint writes on the existing UI/state path so no new background lock is needed. The first slice should only create the recovery-path helper and the checkpoint writer surface; later slices can add startup recovery prompts and shortcut customization.

**Tech Stack:** Rust, egui, tokio, serde, native desktop automation, Windows UI Automation.

---

## Chunk 1: Recovery Path Helper

**Files:**
- Modify: `crates/clipstrike-ui/src/app_config.rs`
- Test: `crates/clipstrike-ui/src/app_config.rs`

- [x] **Step 1: Write a failing unit test for deterministic recovery path derivation from the current project path.**
- [x] **Step 2: Implement the smallest helper that returns a recovery checkpoint path for current or untitled projects.**
- [x] **Step 3: Run the focused unit tests and `cargo check --all-targets`.**

## Chunk 2: Checkpoint Writer

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`
- Test: `crates/clipstrike-ui/src/ui.rs`

- [x] **Step 1: Write a failing test that proves autosave-enabled sessions emit a recovery checkpoint on demand.**
- [x] **Step 2: Implement the minimal checkpoint writer using the existing save path and recovery helper.**
- [x] **Step 3: Verify the UI tests and `cargo check --all-targets`.**

## Chunk 3: Recovery Surface

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`
- Modify: `crates/clipstrike-ui/tests/native_smoke.rs`
- Test: `crates/clipstrike-ui/tests/native_smoke.rs`

- [x] **Step 1: Add a desktop smoke test that verifies recovery state is discoverable after startup.**
- [x] **Step 2: Implement the smallest startup banner/prompt for an available recovery checkpoint.**
- [x] **Step 3: Re-run the native desktop suite and `cargo check --all-targets`.**




