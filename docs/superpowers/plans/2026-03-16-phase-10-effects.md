# Phase 10: Expanded Effects Library Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement high-performance Gaussian blur, Pixelate, and Color Correction effects to match OpenShot's 1:1 parity.

**Architecture:**
1.  **Refactor `fx.wgsl`**: Implement an optimized two-pass Gaussian blur and add Pixelate logic.
2.  **Update `FxUniforms`**: Include `pixelate_size` and `color_tint`.
3.  **UI Integration**: Add sliders for the new effects in the Properties panel.

**Tech Stack:** Rust, WGSL, wgpu.

---

## Chunk 1: Shader Enhancements (Pixelate & Better Blur)

### Task 1: Update fx.wgsl with New Effects

**Files:**
- Modify: `crates/clipstrike-render/src/shaders/fx.wgsl`

- [ ] **Step 1: Implement Pixelate logic by quantizing `coords`.**

```wgsl
// In fx.wgsl
let pixel_size = uniforms.pixelate_size;
if (pixel_size > 1.0) {
    let p_coords = vec2<f32>(coords) / pixel_size;
    let sampled_coords = vec2<i32>(floor(p_coords) * pixel_size);
    // sample from sampled_coords
}
```

- [ ] **Step 2: Add Color Tint (Color Correction).**

- [ ] **Step 3: Verify compilation.**

Run: `cargo check -p clipstrike-render`
Expected: PASS

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-render/src/shaders/fx.wgsl
git commit -m "feat: implement Pixelate and Color Correction in WGSL"
```

---

## Chunk 2: Uniform & Rust Bridge

### Task 2: Update FxUniforms and Rendering Logic

**Files:**
- Modify: `crates/clipstrike-render/src/fx.rs`
- Modify: `crates/clipstrike-core/src/timeline.rs`

- [ ] **Step 1: Add `pixelate_size` and `color_tint` to `FxUniforms`.**

- [ ] **Step 2: Add `pixelate` and `tint` to `Clip` keyframed properties.**

- [ ] **Step 3: Commit.**

```bash
git add crates/clipstrike-render/src/fx.rs crates/clipstrike-core/src/timeline.rs
git commit -m "feat: add Pixelate and Tint properties to FxUniforms and Clip"
```

---

## Chunk 3: UI Controls

### Task 3: Add Effect Controls to Properties Panel

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`

- [ ] **Step 1: Add sliders for Pixelate Size and Color Correction.**

- [ ] **Step 2: Verify real-time updates in the preview.**

- [ ] **Step 3: Commit.**

```bash
git add crates/clipstrike-ui/src/ui.rs
git commit -m "feat: add UI controls for Pixelate and Color Correction"
```
