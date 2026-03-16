# Phase 12: Advanced Audio Engine Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a professional-grade audio engine with multi-track mixing, volume keyframing, and stereo support.

**Architecture:**
1.  **Refactor `AudioSource`**: Include `KeyframedProperty` for volume.
2.  **Upgrade `PlaybackEngine`**: Enhance the mixing loop to support stereo and per-sample volume interpolation.
3.  **GStreamer Integration**: Configure `VideoEngine` to capture and feed audio buffers into the `PlaybackEngine`.
4.  **UI Sync**: Update `sync_engines_for_playhead` to use real samples from GStreamer.

**Tech Stack:** Rust, cpal, GStreamer, clipstrike-core (keyframes).

---

## Chunk 1: Audio Model Upgrades

### Task 1: Add Keyframed Volume to AudioSource

**Files:**
- Modify: `crates/clipstrike-audio/src/lib.rs`

- [ ] **Step 1: Update `AudioSource` struct to use `KeyframedProperty`.**

```rust
// In crates/clipstrike-audio/src/lib.rs
use clipstrike_core::keyframes::KeyframedProperty;

pub struct AudioSource {
    pub clip_id: usize,
    pub samples: Arc<Vec<f32>>,
    pub start_ms: i64,
    pub end_ms: i64,
    pub volume: KeyframedProperty, // Changed from f32
}
```

- [ ] **Step 2: Update `PlaybackEngine` mixing loop to interpolate volume.**

- [ ] **Step 3: Verify compilation.**

Run: `cargo check -p clipstrike-audio`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-audio/src/lib.rs
git commit -m "feat: add keyframed volume support to AudioSource"
```

---

## Chunk 2: Stereo Mixing & Performance

### Task 2: Implement Stereo Mixing Loop

**Files:**
- Modify: `crates/clipstrike-audio/src/lib.rs`

- [ ] **Step 1: Update the mixing loop to handle interleaved stereo samples.**

- [ ] **Step 2: Add master volume smoothing to prevent clicking.**

- [ ] **Step 3: Verify with unit tests.**

Run: `cargo test -p clipstrike-audio`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-audio/src/lib.rs
git commit -m "feat: implement stereo mixing and volume smoothing"
```

---

## Chunk 3: GStreamer Audio Integration

### Task 3: Capture Audio Buffers in VideoEngine

**Files:**
- Modify: `crates/clipstrike-media/src/engine.rs`

- [ ] **Step 1: Add an `appsink` for audio to the GStreamer pipeline.**

- [ ] **Step 2: Implement a callback to collect audio samples into a buffer.**

- [ ] **Step 3: Verify compilation.**

Run: `cargo check -p clipstrike-media --features gstreamer-backend`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-media/src/engine.rs
git commit -m "feat: capture audio buffers from GStreamer pipeline"
```
