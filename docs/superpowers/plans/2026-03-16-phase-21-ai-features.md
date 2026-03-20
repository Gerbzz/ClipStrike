# Phase 21: AI Features Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:execoting-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement UI controls and shader logic for Video Stabilization and Motion Tracking to match OpenShot's advanced features.

**Architecture:**
1.  **UI Controls**: Add toggles and parameters for AI features in the Properties panel.
2.  **Stabilization Shader**: Create a WGSL shader that applies smoothing transforms to video frames.
3.  **Tracking Infrastructure**: Build the foundational data structures to store motion tracking paths.

**Tech Stack:** Rust, WGSL, wgpu.

---

## Chunk 1: AI UI Controls

### Task 1: Add Stabilization & Tracking UI

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`

- [ ] **Step 1: Add a "Video AI" section to the Properties panel.**

- [ ] **Step 2: Add toggles for `stabilization_enabled` and `motion_tracking_enabled`.**

- [ ] **Step 3: Add a slider for "Stabilization Strength".**

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-ui/src/ui.rs
git commit -m "feat: add UI controls for Stabilization and Motion Tracking"
```

---

## Chunk 2: Stabilization Shader

### Task 2: Implement Stabilization in Render Pipeline

**Files:**
- Create: `crates/clipstrike-render/src/shaders/stabilization.wgsl`
- Modify: `crates/clipstrike-render/src/fx.rs`

- [ ] **Step 1: Implement a basic 2D translation stabilization shader.**

- [ ] **Step 2: Integrate the stabilization pass into `FxPipeline`.**

- [ ] **Step 3: Verify compilation.**

Run: `cargo check -p clipstrike-render`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-render/src/shaders/stabilization.wgsl crates/clipstrike-render/src/fx.rs
git commit -m "feat: implement basic video stabilization shader"
```
