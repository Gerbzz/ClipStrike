# ⚡ ClipStrike — Current Implementation State

This document summarizes every feature built in ClipStrike so far, organized by architectural component.

## 1. Core Architecture (Rust)
- **Multi-Crate Workspace:** Logical separation into `core`, `ui`, `media`, `audio`, `render`, and `app`.
- **Interval Tree Timeline:** O(log N) lookup for clips, ensuring performance with thousands of assets.
- **Spatial Hash Snapping:** High-performance magnetic snapping for timeline editing.
- **Async Project Engine:** Non-blocking message passing between UI and core engine via `tokio` channels.
- **Undo/Redo System:** Robust command-pattern implementation for project-wide history.
- **JSON Serialization:** Full `serde` support for `.cstrike` project files (Save/Load verified).

## 2. Rendering & UI (wgpu / egui)
- **WGPU Foundation:** Hardware-accelerated rendering with graceful fallback for testing.
- **Video Cache (Task 6.8):** Background pre-rendering ring buffer (60 frames) ensures buttery smooth playback when playhead is stationary or loop-caching.
- **Hardware-Accelerated GStreamer Pipeline (Task 2.2):** Decoder preference is explicit, decoded frame availability is tracked on the GPU side, the backend now recognizes GL-backed samples and preserves external texture ids, the native preview sink now negotiates a GLMemory-backed path first, and the request builder now keeps CPU fallback pixels alongside metadata so preview stays correct until true external-texture import exists.
- **EGUI Layout:** 1:1 OpenShot UI clone implemented with dockable panels.
- **Texture Pool:** VRAM-efficient texture recycling for real-time video playback.
- **Playback Boundary Warm-Up:** The UI now prewarms the next clip on each track before the playhead reaches it, which reduces decoder startup stalls at clip boundaries.
- **Compute Shader FX:** WGSL-based effects pipeline (Brightness, Blur, Opacity, Transform) plus advanced compositor blend modes (Multiply, Screen, Overlay, Difference, Hue, Saturation, Color, Luminosity, Add, Subtract).
- **Icon/Font Polish:** Unicode glyph support and 15% font size increase for better readability.
- **Advanced Preferences:** External executable path configuration for Blender/Inkscape with persisted app-level settings.
- **Session Recovery:** Autosave-enabled editor mutations now write deterministic `.cstrike.recovery` checkpoints through the UI save path helper, using the current project path when available or an untitled recovery path otherwise.

## 3. Editing Features
- **Track Management:** Unlimited dynamic tracks with Add/Remove/Rename/Undo support.
- **Clip Manipulation:**
    - Drag-and-drop from Media Browser to Timeline.
    - Timeline repositioning (horizontal and cross-track).
    - Interactive Razor (Slice) tool.
    - Edge-handle trimming.
    - Clip Copy/Paste/Delete.
- **Transport Controls:** Play/Pause, Seeking, and frame-accurate Stepping (Next/Prev).
- **Keyframe Engine:** Linear, Bézier, and Constant interpolation for all clip properties.
- **Automatic Transitions:** Detection and generation of crossfades when clips overlap.
- **Speed Control:** Support for multipliers (2x, 0.5x), Reverse playback, and Freeze Frame.

## 4. Media & Browser
- **Media Browser:** Basic import references, search filtering, and type-based filtering.
- **Asset Management:** Support for Video, Audio, and Image asset references.
- **Title System:** Built-in SVG title editor with data URI support.
- **Marker System:** Add, label, navigate, remove, and drag timeline markers directly on the ruler.

## 5. Testing & Quality
- **Native Regression Tests:** Rust tests cover core timeline math, snapping, keyframes, and UI command handling.
- **Headless Verification:** In-process harnesses cover the desktop add/import and timeline flows without needing a physical monitor.
- **Native Desktop Smoke Coverage:** `egui_kittest` and Windows UI Automation exercise the real Import button, imported media rows, and row-level `+` add-to-timeline actions using fixture media with Windows-safe file names.

- **Preview Fast Path:** External-backed frames now bypass the FX pass when the clip is visually neutral, preserving the external-texture metadata bridge without adding CPU readback.

- **Save-Frame Readback Reuse:** The native save-frame path now fills a caller-owned CPU buffer and writes it through `image::save_buffer`, so repeated frame exports can reuse the same allocation instead of consuming a fresh `Vec<u8>` each time.

- **Thumbnail Readback Reuse:** The GStreamer thumbnail fallback now has a caller-owned buffer helper (`generate_thumbnail_sync_into`) so repeated thumbnail reads can reuse the same allocation while preserving the owned wrapper for existing call sites.
