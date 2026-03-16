# Phase 6: Video Cache Thread Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a background Video Cache Thread to pre-render composited frames into RAM/VRAM for smooth playback, matching OpenShot's performance features.

**Architecture:**
1.  **Cache Storage**: Create a `VideoCache` in `clipstrike-render` to hold pre-rendered textures.
2.  **Background Worker**: Implement a thread in `EngineManager` that watches the playhead and renders upcoming frames.
3.  **UI Integration**: Check the cache in `ClipStrikeApp::update` and display cache progress in the timeline.

**Tech Stack:** Rust, wgpu, tokio.

---

## Chunk 1: Cache Infrastructure

### Task 1: Implement VideoCache in clipstrike-render

**Files:**
- Create: `crates/clipstrike-render/src/cache.rs`
- Modify: `crates/clipstrike-render/src/lib.rs`

- [ ] **Step 1: Define `VideoCache` struct and its storage mechanism.**

```rust
// In crates/clipstrike-render/src/cache.rs
pub struct VideoCache {
    pub frames: HashMap<i64, FrameTexture>, // time_ms -> texture
}
```

- [ ] **Step 2: Add methods to insert and retrieve cached frames.**

- [ ] **Step 3: Verify compilation.**

Run: `cargo check -p clipstrike-render`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-render/src/cache.rs
git commit -m "feat: implement VideoCache infrastructure"
```

---

## Chunk 2: Background Pre-rendering

### Task 2: Implement Cache Worker in EngineManager

**Files:**
- Modify: `crates/clipstrike-media/src/engine_manager.rs`

- [ ] **Step 1: Add a background task that pre-renders frames ahead of the playhead.**

- [ ] **Step 2: Use the `CompositorPipeline` to render into the cache.**

- [ ] **Step 3: Implement cache eviction (LUR/FIFO) to prevent memory bloat.**

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-media/src/engine_manager.rs
git commit -m "feat: implement background video cache worker"
```

---

## Chunk 3: UI Integration

### Task 3: Use Cache for Playback and Display Status

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`

- [ ] **Step 1: Update the rendering loop to prefer cached frames.**

- [ ] **Step 2: Add a cache status bar to the timeline UI.**

- [ ] **Step 3: Commit.**

```bash
git add crates/clipstrike-ui/src/ui.rs
git commit -m "feat: integrate video cache into UI and display status"
```
