# Specification: Advanced Compositing (Track 008)

## Overview
Implement advanced compositing features including watermarks, solid color generators (for backgrounds), and interactive on-canvas cropping/transform handles.

## Goals
- **Watermarks:** Support persistent overlays that apply to the entire project or specific clips.
- **Generators:** Add a system for generating solid colors, gradients, and test patterns natively on the GPU.
- **Interactive Handles:** Implement on-canvas UI handles for cropping and transforming clips directly in the preview window.

## Requirements

### 1. Watermark Support
- Allow users to designate an image asset as a "Watermark".
- Watermarks should have global properties (position, opacity, scale).
- Support for "Always on Top" rendering in the compositor.

### 2. Solid Color Generators
- Add a new asset type: `GeneratorAsset`.
- Implement a GPU-based solid color generator.
- Generators should be treated as "Infinite" duration assets that can be trimmed on the timeline.

### 3. Interactive UI Handles
- Draw transform handles (corners and edges) over the selected clip in the preview.
- Support dragging handles to update `scale_x`, `scale_y`, `pos_x`, and `pos_y` properties.
- Implement a "Crop" mode with dedicated handles.

## Technical Architecture
- **Shaders:** Update `compositor.wgsl` to handle global watermark overlays.
- **Render Engine:** Add a `GeneratorPipeline` for creating textures from parameters (colors, gradients).
- **UI:** Implement a `TransformOverlay` in `clipstrike-ui` that translates canvas coordinates to project coordinates.
