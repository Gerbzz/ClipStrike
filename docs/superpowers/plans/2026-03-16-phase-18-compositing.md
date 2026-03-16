# Phase 18: Professional Compositing Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement 16+ professional blend modes to match OpenShot's compositing capabilities.

**Architecture:**
1.  **Refactor `BlendMode`**: Assign explicit integer values to each enum variant.
2.  **Upgrade `compositor.wgsl`**: Implement the missing 10+ blend modes (Difference, Hue, Saturation, etc.).
3.  **UI Integration**: Ensure the Properties panel correctly updates the `Clip`'s `blend_mode` and triggers a re-render.

**Tech Stack:** Rust, WGSL, wgpu.

---

## Chunk 1: Blend Mode Enum & Mapping

### Task 1: Update BlendMode with Explicit Values

**Files:**
- Modify: `crates/clipstrike-core/src/timeline.rs`
- Modify: `crates/clipstrike-render/src/fx.rs`

- [ ] **Step 1: Assign integer values to `BlendMode` variants.**

```rust
// In crates/clipstrike-core/src/timeline.rs
pub enum BlendMode {
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Overlay = 3,
    Darken = 4,
    Lighten = 5,
    // ... add all others ...
    Add = 16,
    Subtract = 17,
}
```

- [ ] **Step 2: Add a conversion from `BlendMode` to `u32` for `ComposeUniforms`.**

- [ ] **Step 3: Verify compilation.**

Run: `cargo check -p clipstrike-core`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-core/src/timeline.rs
git commit -m "feat: assign explicit integer values to BlendMode"
```

---

## Chunk 2: WGSL Shader Implementation

### Task 2: Implement Missing Blend Modes in compositor.wgsl

**Files:**
- Modify: `crates/clipstrike-render/src/shaders/compositor.wgsl`

- [ ] **Step 1: Implement the math for Difference, Exclusion, Color Dodge, etc.**

- [ ] **Step 2: Update the `switch` statement in `main`.**

- [ ] **Step 3: Verify with `cargo check` (shader compilation is checked during pipeline creation).**

Run: `cargo check -p clipstrike-render`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-render/src/shaders/compositor.wgsl
git commit -m "feat: implement professional blend modes in WGSL"
```

---

## Chunk 3: UI & Preview Synchronization

### Task 3: Finalize UI Selection and Preview

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`

- [ ] **Step 1: Ensure the blend mode dropdown in the Properties panel works correctly.**

- [ ] **Step 2: Verify that changing blend mode triggers a timeline update.**

- [ ] **Step 3: Commit.**

```bash
git add crates/clipstrike-ui/src/ui.rs
git commit -m "feat: enable professional blend mode selection in UI"
```
