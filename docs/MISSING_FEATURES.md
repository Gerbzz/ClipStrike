# 🚩 ClipStrike — Missing Features & Gap Analysis

This document identifies the features required to achieve 1:1 parity with OpenShot 3.4 and outlines the technical roadmap for their implementation.

## 1. High-Priority Infrastructure
- **Video Preview Render (Native):** Complete the GStreamer pipeline integration to decode and display real video frames in the `wgpu` canvas.
- **Audio Engine (Native):** Implement `cpal` or GStreamer audio sink for real-time sound playback synchronized with the video.
- **Advanced Export Profiles:** Build a preset system for standard output formats (YouTube, 4K, Blu-ray).

## 2. Video Effects & Compositing
- **Professional FX:**
    - **Chroma Key:** WGSL compute shader for green/blue screen removal.
    - **Color Map (LUT):** Support for importing and applying `.cube` LUT files.
    - **Gamma & Hue:** Additional color grading shaders.
    - **Sharpen:** High-pass filter compute shader.
- **Blend Modes:** Implement Multiply, Screen, Overlay, and Subtract blending logic in the render pipeline.
- **Interactive Cropping:** Add visual handles to the video preview for direct canvas-based cropping.

## 3. Titles & External Integration
- **3D Animated Titles:** Implement a background bridge to Blender for rendering 3D title templates.
- **SVG Templates:** Expand the title library with professional SVG presets.
- **External Bridge:** Add "Edit in Inkscape" and "Edit in Blender" right-click commands that watch for file changes.

## 4. Advanced Tooling
- **AI & Computer Vision:**
    - **Video Stabilization:** Integrated tracking and transformation logic.
    - **Motion Tracking:** UI and logic for following objects across frames.
    - **Object Detection:** Automatic bounding box generation.
- **Video Cache:** Background thread to pre-render composited frames into RAM for smooth playback.
- **Auto-Recovery:** Implement session checkpointing to protect against crashes.

## 5. UI/UX Refinement
- **Localization:** Integrate `rust-i18n` and support 70+ languages.
- **Advanced Preferences:** Keyboard shortcut customization and external tool path configuration (Blender/Inkscape).
- **Keyboard Shortcuts:** Implement full coverage for timeline editing (Razor, Trim, Snapping toggle).

---

## 🛠️ Implementation Plan (Phase 17 - 21)

### Phase 17: Format & Codec Support
- [ ] Verify full FFmpeg decoder support via GStreamer.
- [ ] Implement robust Image Sequence import logic.

### Phase 18: Compositing Modes
- [x] Add `BlendMode` enum to `Clip` and `Transition`.
- [x] Implement multi-mode blending in `fx.wgsl`.

### Phase 19: Markers UI
- [x] Add draggable markers directly on the Time Ruler.
- [x] Add Right-click -> "Add Marker" at any point on the ruler.

### Phase 20: Preferences Logic
- [x] Implement External Executables tab for Blender/Inkscape absolute paths.
- [ ] Implement App-level JSON config storage.
- [ ] Build the Shortcut Customization UI.

### Phase 21: AI Features (The Final Frontier)
- [ ] Integrate a lightweight tracking library or custom WGSL tracking shader.
- [x] Build the stabilization UI controls in the Properties panel.


