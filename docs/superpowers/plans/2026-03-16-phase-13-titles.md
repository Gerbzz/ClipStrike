# Phase 13: Advanced Title System Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand the title system with more templates and support for custom SVG imports to achieve 1:1 parity with OpenShot.

**Architecture:**
1.  **Expand `TitleTemplate`**: Add professional presets (End Credits, Subtitle, Watermark).
2.  **Upgrade SVG Generator**: Implement complex SVG layouts for new templates.
3.  **SVG Import**: Build the logic to import external SVG files as `TitleAsset`.

**Tech Stack:** Rust, egui, SVG.

---

## Chunk 1: Template Expansion

### Task 1: Add New Title Templates

**Files:**
- Modify: `crates/clipstrike-core/src/project.rs`
- Modify: `crates/clipstrike-ui/src/ui.rs`

- [ ] **Step 1: Add new variants to `TitleTemplate`.**

```rust
// In crates/clipstrike-core/src/project.rs
pub enum TitleTemplate {
    Centered,
    LowerThird,
    TopLeft,
    EndCredits, // New
    Subtitle,   // New
    Watermark,  // New
}
```

- [ ] **Step 2: Update UI to expose new templates in the dropdown.**

- [ ] **Step 3: Update SVG generation logic in `draw_title_editor`.**

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-core/src/project.rs crates/clipstrike-ui/src/ui.rs
git commit -m "feat: add End Credits, Subtitle, and Watermark title templates"
```

---

## Chunk 2: SVG Import System

### Task 2: Implement External SVG Import

**Files:**
- Modify: `crates/clipstrike-ui/src/ui.rs`

- [ ] **Step 1: Add "Import SVG Title" button to the Titles panel.**

- [ ] **Step 2: Use `rfd` (File Dialog) to select SVG files.**

- [ ] **Step 3: Parse SVG and create a `TitleAsset` with its content.**

- [ ] **Step 4: Commit.**

```bash
git add crates/clipstrike-ui/src/ui.rs
git commit -m "feat: implement custom SVG title import"
```
