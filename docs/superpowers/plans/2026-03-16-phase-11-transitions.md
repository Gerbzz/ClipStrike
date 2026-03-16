# Phase 11: Professional Transitions Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement high-performance WGSL-based transitions to match OpenShot's professional library.

**Architecture:**
1.  **Refactor Transitions**: Move mask sampling logic from CPU to a new `transition.wgsl` shader.
2.  **Expand Library**: Add `RadialWipe`, `MatrixWipe`, and `SpiralWipe` to `TransitionType`.
3.  **UI Preview**: Ensure the timeline correctly previews these transitions in real-time.

**Tech Stack:** Rust, WGSL, wgpu.

---

## Chunk 1: Transition Shader

### Task 1: Create transition.wgsl

**Files:**
- Create: `crates/clipstrike-render/src/shaders/transition.wgsl`
- Modify: `crates/clipstrike-render/src/fx.rs`

- [ ] **Step 1: Implement the base transition shader with support for Wipes and Clock.**

```wgsl
// In transition.wgsl
@compute @workgroup_size(16, 16)
fn main(...) {
    // Port logic from sample_mask_value to GPU
}
```

- [ ] **Step 2: Create `TransitionPipeline` in `fx.rs`.**

- [ ] **Step 3: Verify compilation.**

Run: `cargo check -p clipstrike-render`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-render/src/shaders/transition.wgsl crates/clipstrike-render/src/fx.rs
git commit -m "feat: implement high-performance transition shader"
```

---

## Chunk 2: Expanded Transition Library

### Task 2: Add New Transition Types

**Files:**
- Modify: `crates/clipstrike-core/src/transitions.rs`

- [ ] **Step 1: Add `RadialWipe`, `MatrixWipe`, `SpiralWipe` to `TransitionType`.**

- [ ] **Step 2: Update `label()` and `uses_luma_mask()`.**

- [ ] **Step 3: Commit.**

```bash
git add crates/clipstrike-core/src/transitions.rs
git commit -m "feat: add Radial, Matrix, and Spiral wipes"
```

---

## Chunk 3: UI Integration

### Task 3: Enable Transition Selection in UI

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`

- [ ] **Step 1: Update the transition creation context menu with new types.**

- [ ] **Step 2: Verify real-time transition preview on the canvas.**

- [ ] **Step 3: Commit.**

```bash
git add crates/clipstrike-ui/src/ui.rs
git commit -m "feat: expose professional transitions in the UI"
```
