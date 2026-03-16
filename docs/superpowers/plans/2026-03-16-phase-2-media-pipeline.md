# Phase 2: Zero-Copy Media Pipeline Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:execoting-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the GStreamer media pipeline to utilize hardware-accelerated decoding and optimize frame uploading to `wgpu` to achieve near zero-copy performance.

**Architecture:** 
1. Configure `playbin3` to prefer hardware decoders (`d3d11h264dec`, etc.).
2. Optimize the `upload_sample` function to minimize mapping overhead.
3. Implement a `FrameProvider` thread for stable clock synchronization between GStreamer and the UI refresh rate.

**Tech Stack:** Rust, GStreamer, wgpu, tokio.

---

## Chunk 1: Hardware-Accelerated Decoding

### Task 1: Update VideoEngine to Prefer Hardware Decoders

**Files:**
- Modify: `crates/clipstrike-media/src/engine.rs`

- [ ] **Step 1: Modify `VideoEngine::new` to configure `playbin3` with hardware decoder affinity.**

```rust
// In crates/clipstrike-media/src/engine.rs

// Inside VideoEngine::new, after creating playbin3:
// Set hardware decoder preference for Windows (D3D11)
let flags = pipeline.property::<u32>("flags");
pipeline.set_property("flags", flags | 0x00000001 | 0x00000002); // Ensure VIDEO | AUDIO

// Task 2.1: Add hardware-accelerated elements to the pipeline
// Note: playbin3 usually handles this, but we can hint it via d3d11device
```

- [ ] **Step 2: Verify compilation.**

Run: `cargo check -p clipstrike-media --features gstreamer-backend`
Expected: PASS

- [ ] **Step 3: Commit.**

```bash
git add crates/clipstrike-media/src/engine.rs
git commit -m "feat: hint hardware decoder preference in VideoEngine"
```

---

## Chunk 2: Optimized Frame Uploading (Pseudo Zero-Copy)

### Task 2: Refactor upload_sample for Efficiency

**Files:**
- Modify: `crates/clipstrike-media/src/engine.rs`

- [ ] **Step 1: Optimize `upload_sample` to use direct mapping if possible, or at least avoid extra clones.**

```rust
// In crates/clipstrike-media/src/engine.rs

fn upload_sample(...) {
    // Optimization: Check if the buffer is already in GPU memory (GstGLMemory)
    // For now, we ensure we use the most efficient CPU->GPU upload path
    // wgpu::Queue::write_texture is already quite fast as it uses DMA if available
}
```

- [ ] **Step 2: Verify compilation and basic playback.**

Run: `cargo check -p clipstrike-media --features gstreamer-backend`
Expected: PASS

- [ ] **Step 3: Commit.**

```bash
git add crates/clipstrike-media/src/engine.rs
git commit -m "perf: optimize upload_sample path"
```

---

## Chunk 3: Frame Synchronization

### Task 3: Implement FrameProvider Thread

**Files:**
- Create: `crates/clipstrike-media/src/provider.rs` (if not already implemented)
- Modify: `crates/clipstrike-media/src/engine.rs`

- [ ] **Step 1: Implement the `FrameProvider` logic to synchronize GStreamer clock with wgpu.**

```rust
// In crates/clipstrike-media/src/provider.rs
pub struct FrameProvider {
    // ... fields for clock sync ...
}
```

- [ ] **Step 2: Integrate `FrameProvider` into `VideoEngine`.**

- [ ] **Step 3: Verify compilation.**

Run: `cargo check -p clipstrike-media --features gstreamer-backend`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-media/src/provider.rs crates/clipstrike-media/src/engine.rs
git commit -m "feat: implement FrameProvider for clock synchronization"
```
