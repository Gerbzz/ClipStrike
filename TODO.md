# 🎯 ClipStrike — Full 1:1 OpenShot Clone (Rust / wgpu / GStreamer)

This document maps **every major OpenShot feature** into ClipStrike's Rust architecture.
Phases are ordered by dependency — later phases build on earlier ones.

---

## Phase 0: Build & Toolchain ⚙️
Goal: Get the project compiling on Windows with all backends.
- [x] **Task 0.1:** Install GStreamer MSVC runtime + dev SDK and set `PKG_CONFIG_PATH` so Cargo can find `gstreamer-1.0.pc`.
- [x] **Task 0.2:** Verify `cargo check --all-features` passes (no pkg-config errors).
- [x] **Task 0.3:** Verify `cargo test` passes (unit tests in `timeline.rs`, `project.rs`).

---

## Phase 1: GPU Foundation & Render Loop 🖥️
Goal: Hardware-accelerated window with a game-style UI loop.
- [x] **Task 1.1:** Setup `eframe` with `wgpu` backend for the primary windowing engine.
- [x] **Task 1.2:** Implement a `TexturePool` to manage and recycle GPU textures (prevent VRAM fragmentation).
- [x] **Task 1.3:** Create a WGSL "Display Shader" for YUV → RGB color space conversion on the GPU.
- [x] **Task 1.4:** Build the core OpenShot 1:1 UI Layout using `egui`.

---

## Phase 2: Zero-Copy Media Pipeline 🎬
Goal: Video playing via GPU decoding (NVDEC/VA-API/Metal) directly into `wgpu`.
- [x] **Task 2.1:** Initialize GStreamer runtime and build a `playbin3` pipeline targeting hardware decoders.
- [x] **Task 2.2:** Configure `glupload` + `appsink` to pass raw GPU pointers to `wgpu` (zero-copy).
- [x] **Task 2.3:** Implement a "Frame Provider" thread for clock synchronization between GStreamer and `wgpu` refresh rate.
- [x] **Task 2.4:** Build a background "Metadata Probe" using `gst-discoverer` for clip info (FPS, resolution, duration).
- [x] **Task 2.5:** Implement **Variable Frame Rate (VFR) Compensation** — dynamically duplicate or drop frames during GStreamer decoding to keep VFR smartphone footage perfectly synced with the timeline audio clock.
- [x] **Task 2.6:** Implement **Hardware Decoder Fallback** — if `playbin3` fails to hardware-decode a specific codec on the GPU, gracefully fallback to CPU decoding without crashing the application.

---

## Phase 3: The "Game-Engine" Timeline ⏱️
Goal: Manage thousands of clips with sub-millisecond seek times.
- [x] **Task 3.1:** Implement an **Interval Tree** for O(log N) queries of "what clip is at time X".
- [x] **Task 3.2:** Implement **Timeline Virtualization** — only render clips within the current scroll viewport.
- [x] **Task 3.3:** Build a "Snap Engine" using a spatial hash for magnetic clip-edge snapping.
- [x] **Task 3.4:** Implement **Razor (Slice)** — split a clip at the playhead into two halves.
- [x] **Task 3.5:** Implement **Trim** — drag left/right edges of a clip to change its in/out points.
- [x] **Task 3.6:** Implement **Multi-track Layering** (Z-order) — clips on higher tracks render on top.


---

## Phase 4: Clip Manipulation & Drag-and-Drop 🖱️
Goal: Full interactive clip editing matching OpenShot's timeline behavior.
- [ ] **Task 4.1:** Implement **drag-and-drop** of clips from the media browser onto timeline tracks.
- [x] **Task 4.2:** Implement **clip drag/move** on the timeline (horizontal repositioning + cross-track moves).
- [ ] **Task 4.3:** Implement **clip resize** via edge handles (visual trim).
- [x] **Task 4.4:** Implement **clip copy/paste/delete** with keyboard shortcuts.
- [x] **Task 4.5:** Implement **clip right-click context menu** (Properties, Remove, Copy, Slice, etc.).
- [x] **Task 4.6:** Implement **Playhead** — draggable position indicator with frame-accurate seeking.
- [x] **Task 4.7:** Implement **Time Ruler** — accurate timecode display (HH:MM:SS:FF) with zoom-dependent tick marks.

---

## Phase 5: Unlimited Tracks & Track Management 🎚️
Goal: Dynamic track system matching OpenShot's unlimited layers.
- [x] **Task 5.1:** Implement **dynamic track creation/deletion** (Add Track / Remove Track).
- [x] **Task 5.2:** Implement **per-track Visibility toggle** (eye icon — hide track from preview & export).
- [x] **Task 5.3:** Implement **per-track Audio Mute/Solo** (speaker icon).
- [x] **Task 5.4:** Implement **per-track Lock** (prevent edits to locked tracks).
- [x] **Task 5.5:** Implement **track reordering** (drag track headers up/down to change Z-order).
- [x] **Task 5.6:** Implement **track rename** (double-click track header to rename).

---

## Phase 6: Video Preview & Transport Controls ▶️
Goal: Full video preview with frame-accurate transport.
- [x] **Task 6.1:** Render composited video preview in the central panel from timeline playback state.
- [x] **Task 6.2:** Implement **Play / Pause** transport button.
- [x] **Task 6.3:** Implement **Jump to Start / Jump to End**.
- [x] **Task 6.4:** Implement **Previous Frame / Next Frame** (frame-stepping).
- [x] **Task 6.5:** Implement **Timecode Display** — current time and total duration (HH:MM:SS:FF).
- [x] **Task 6.6:** Implement **Save Current Frame** as an image file (screenshot export).
- [x] **Task 6.7:** Implement **looped playback** (Loop In/Out markers).
- [ ] **Task 6.8:** Implement a background **Video Cache Thread** — pre-render upcoming composited frames into RAM while the timeline is paused to ensure buttery-smooth playback.

---

## Phase 7: File Import & Media Browser 📁
Goal: Full media management matching OpenShot's Project Files panel.
- [x] **Task 7.1:** Implement **File Import Dialog** (`rfd` native file picker) for video, audio, and image files.
- [ ] **Task 7.2:** Implement **drag-and-drop import** from the OS file explorer into the Project Files panel.
- [ ] **Task 7.3:** Generate and display **thumbnail previews** for imported media files.
- [x] **Task 7.4:** Implement **media type filter** combo box (All / Video / Audio / Image).
- [x] **Task 7.5:** Implement **search/filter** text box for the media browser.
- [x] **Task 7.6:** Implement **Remove File** from project (with confirmation).
- [x] **Task 7.7:** Store imported media references in the `.cstrike` project file.
- [ ] **Task 7.8:** Implement a **"Missing File" Relinker** dialog — on project load, if a `.cstrike` asset path is invalid, prompt the user to locate the folder and automatically relink all missing media.
- [ ] **Task 7.9:** Implement built-in **Generators** — a tab for built-in solid colors, SMPTE test patterns, white noise, and standard open-source Emojis/Vectors.

---

## Phase 8: Properties Panel & Clip Inspector 🔧
Goal: Per-clip property editor matching OpenShot's Properties dock.
- [x] **Task 8.1:** Create the **Properties Panel** as a dockable side panel.
- [x] **Task 8.2:** Display basic clip properties: Position (X, Y), Scale, Rotation, Opacity/Alpha.
- [x] **Task 8.3:** Display clip timing: Track Position, In Point, Out Point, Duration.
- [x] **Task 8.4:** Display source info: File path, Resolution, FPS, Codec.
- [x] **Task 8.5:** Make all numeric properties **editable** with immediate preview update.
- [ ] **Task 8.6:** Implement **volume slider** for audio properties of video/audio clips.

---

## Phase 9: Keyframe Animation System 🎞️
Goal: Full curve-based keyframe animation matching OpenShot's keyframe engine.
- [x] **Task 9.1:** Design the **Keyframe data model** — keyframe list with (time, value, interpolation mode).
- [x] **Task 9.2:** Implement **Linear interpolation** between keyframes.
- [x] **Task 9.3:** Implement **Bézier (quadratic) interpolation** between keyframes.
- [x] **Task 9.4:** Implement **Constant (hold) interpolation** between keyframes.
- [x] **Task 9.5:** Build a **Keyframe Curve Editor UI** — visual graph for editing keyframe curves.
- [x] **Task 9.6:** Allow **any clip property** (position, scale, rotation, opacity, volume) to be keyframed.
- [x] **Task 9.7:** Implement **keyframe snapping** to the playhead and other keyframes.


---

## Phase 10: Compute Shader Effects (FX Core) ✨
Goal: All filters run as WGSL Compute Shaders on the GPU.
- [x] **Task 10.1:** Create a `ComputePipeline` architecture for layering multiple shaders on a single frame without copying back to RAM.
- [x] **Task 10.2:** **Brightness / Contrast / Saturation** (Color Grading shader).
- [ ] **Task 10.3:** **Gamma correction** shader.
- [ ] **Task 10.4:** **Hue / Color Shift** shader.
- [ ] **Task 10.5:** **Greyscale / Desaturation** shader.
- [ ] **Task 10.6:** **Chroma Key** (greenscreen / bluescreen removal) shader.
- [x] **Task 10.7:** **Alpha Blending / Opacity** transitions shader.
- [x] **Task 10.8:** **Transform** (Scale, Rotate, Position) via `glam` math on the GPU.
- [x] **Task 10.9:** **Blur** (Gaussian) compute shader.
- [ ] **Task 10.10:** Build the **Effects Browser** panel — list all available effects with categories.
- [ ] **Task 10.11:** Implement **drag-and-drop effects onto clips** on the timeline.
- [x] **Task 10.12:** All effect parameters must be **keyframeable** (connected to Phase 9).
- [ ] **Task 10.13:** **LUT Color Map** shader — support importing `.cube` files for cinematic color grading.
- [ ] **Task 10.14:** **Lens Flare & Outline** compute shaders.
- [ ] **Task 10.15:** **Spherical / 360 Video** mapping shader (converting equirectangular to standard view).

---

## Phase 11: Transitions 🔀
Goal: Smooth transitions between clips matching OpenShot's 400+ presets.
- [x] **Task 11.1:** Implement **auto-transition** — overlapping two clips on the same track creates a crossfade.
- [x] **Task 11.2:** Implement **Fade In / Fade Out** (opacity-based).
- [x] **Task 11.3:** Implement **Crossfade / Dissolve** between two clips.
- [x] **Task 11.4:** Implement **Wipe transitions** using grayscale luma masks (left, right, up, down, diagonal, clock, etc.).
- [ ] **Task 11.5:** Create the **Transitions Browser** panel with categories and thumbnail previews.
- [ ] **Task 11.6:** Implement **drag-and-drop** transitions between clips.
- [x] **Task 11.7:** Implement **custom luma mask import** for user-defined transitions.
- [x] **Task 11.8:** Transition **speed/sharpness** must be keyframeable.

---

## Phase 12: Audio Engine 🔊
Goal: Full audio playback, mixing, and waveform display.
- [ ] **Task 12.1:** Integrate an audio output backend (e.g., `cpal` or GStreamer audio sink).
- [x] **Task 12.2:** Implement **audio playback** synchronized with video preview.
- [x] **Task 12.3:** Generate and display **audio waveforms** on timeline clips.
- [x] **Task 12.4:** Implement **volume and pan keyframes** per clip.
- [ ] **Task 12.5:** Implement **audio mixing** — combine all active tracks into a stereo output.
- [ ] **Task 12.6:** Implement **Split Audio from Video** — extract audio track as a separate timeline clip.
- [ ] **Task 12.7:** Implement **per-channel audio adjustment** (left/right balance).
- [ ] **Task 12.8:** Implement **Audio Fade In / Fade Out** effects.
- [ ] **Task 12.9:** Implement **Audio Effects Library** — Compressor, Delay, Echo, Parametric EQ, and Robotization filters applied via GStreamer audio plugins or `cpal` DSP processing.

---

## Phase 13: Title System 🏷️
Goal: Title creation matching OpenShot's title editor.
- [ ] **Task 13.1:** Implement a **Title Editor** dialog — text input, font selection, color, size, position.
- [ ] **Task 13.2:** Render titles as **GPU text textures** overlaid on the timeline.
- [ ] **Task 13.3:** Implement **title templates** (lower third, centered, etc.).
- [ ] **Task 13.4:** Implement **SVG vector title import** support.
- [ ] **Task 13.5:** Implement **scrolling credits** (vertical scroll animation).
- [ ] **Task 13.6:** (Stretch) 3D animated title integration (Blender subprocess or simulated).
- [ ] **Task 13.7:** Implement the **Inkscape Bridge** — "Edit in Inkscape" right-click option that opens the SVG in the external app, waits for file modification, and auto-reloads the texture on the timeline.
- [ ] **Task 13.8:** Implement the **Blender Bridge** — execute `blender.exe` in the background with a generated Python script to render 3D animated titles (exploding text, lens flares) to an image sequence, then auto-import that sequence to the timeline.

---

## Phase 14: Time Remapping & Speed Control ⏩
Goal: Per-clip speed manipulation matching OpenShot's time-mapping.
- [x] **Task 14.1:** Implement **clip speed multiplier** (0.25x, 0.5x, 2x, 4x, etc.).
- [x] **Task 14.2:** Implement **reverse playback** for a clip.
- [x] **Task 14.3:** Implement **keyframeable speed curves**  variable speed within a single clip (time remapping).
- [x] **Task 14.4:** Implement **freeze frame** at a specific point in a clip.

---

## Phase 15: Undo/Redo & History 🔄
Goal: Full undo/redo system matching OpenShot's history.
- [x] **Task 15.1:** Design a **Command Pattern** — all timeline edits produce undoable commands.
- [x] **Task 15.2:** Implement an **undo stack** with configurable depth.
- [x] **Task 15.3:** Implement **Redo** (reapply undone commands).
- [x] **Task 15.4:** Implement **Clear History** menu action.
- [x] **Task 15.5:** Wire undo/redo to keyboard shortcuts (Ctrl+Z / Ctrl+Y).

---

## Phase 16: Async Project Engine & Export 📦
Goal: Non-blocking UI and high-performance export.
- [x] **Task 16.1:** Implement `serde`-based JSON serialization for `.cstrike` project files.
- [x] **Task 16.2:** Use `Tokio` channels for UI ↔ Engine message passing (no global mutexes).
- [x] **Task 16.3:** Implement **Save / Save As / Open / New** project workflows wired to UI buttons.
- [x] **Task 16.4:** Implement **Recent Projects** list (stored in app config).
- [x] **Task 16.5:** Create the **Export Pipeline** — feed `wgpu` render output into a GStreamer encoder (MP4, WebM).
- [ ] **Task 16.6:** Implement **Export Profiles** — presets for Full HD (1080p), 4K, 8K, DVD, Blu-ray, Web, etc.
- [ ] **Task 16.7:** Implement **Export Progress** bar with cancel support.
- [x] **Task 16.8:** Implement background **Proxy Generation** using `Rayon` for heavy projects.
- [ ] **Task 16.9:** (Stretch) EDL / Final Cut Pro XML project import/export.

---

## Phase 17: Format & Codec Support 📼
Goal: Broad format support comparable to OpenShot's FFmpeg-backed I/O.
- [x] **Task 17.1:** Support importing common **video** formats: MP4, MKV, AVI, MOV, WebM, AVCHD, HEVC.
- [x] **Task 17.2:** Support importing common **audio** formats: MP3, AAC, WAV, FLAC, OGG.
- [x] **Task 17.3:** Support importing **image** formats: PNG, JPG, BMP, TIFF, SVG (rasterized), GIF.
- [ ] **Task 17.4:** Support importing **image sequences** (numbered frames → clip).
- [ ] **Task 17.5:** Expose codec selection in the Export dialog (H.264, H.265, VP9, AV1, ProRes, etc.).

---

## Phase 18: Compositing & Overlays 🎨
Goal: Advanced compositing matching OpenShot's layering system.
- [x] **Task 18.1:** Implement **alpha compositing** — clips on higher tracks blend over lower ones.
- [ ] **Task 18.2:** Implement **blend modes** (Normal, Multiply, Screen, Overlay, Add, etc.).
- [ ] **Task 18.3:** Implement **image overlay / watermark** support (drag an image into a track).
- [ ] **Task 18.4:** Implement **Solid Color Clips** — create a clip that is a flat color (for backgrounds).
- [ ] **Task 18.5:** Implement **Digital Zoom** — keyframeable crop-and-scale within a clip.

---

## Phase 19: Markers & Annotations 📍
Goal: Timeline markers and notes matching OpenShot's marker system.
- [x] **Task 19.1:** Implement **Add / Remove Marker** at the playhead.
- [x] **Task 19.2:** Implement **Previous Marker / Next Marker** navigation.
- [x] **Task 19.3:** Implement **marker labels** and colors.
- [x] **Task 19.4:** Save and load markers in the `.cstrike` project file.

---

## Phase 20: Preferences & Settings ⚙️
Goal: User preferences matching OpenShot's settings dialog.
- [x] **Task 20.1:** Implement a **Preferences Dialog** (default project settings, auto-save, theme, etc.).
- [ ] **Task 20.2:** Implement **Keyboard Shortcut Customization**.
- [x] **Task 20.3:** Implement **Choose Profile** — set project resolution, FPS, and aspect ratio.
- [x] **Task 20.4:** Implement **Simple / Advanced View** toggle (show/hide panels).
- [x] **Task 20.5:** Implement **Fullscreen toggle** for the main window.
- [x] **Task 20.6:** Implement **App-level configuration file** (JSON, stored in OS appdata).
- [ ] **Task 20.7:** Implement an **External Executables** tab — allow the user to set the absolute OS paths for `blender.exe` and `inkscape.exe`.
- [ ] **Task 20.8:** Implement **Localization / i18n** framework (e.g., `rust-i18n`) — ensure no UI strings are hardcoded in English, matching OpenShot's 70+ language support.
- [ ] **Task 20.9:** Implement **Auto-Recovery UI** — detect if the last session crashed via the auto-save thread and prompt the user to restore it on startup.

---

## Phase 21: AI & Computer Vision (Stretch Goals) 🤖
Goal: Advanced features matching OpenShot 2.6+ AI capabilities.
- [ ] **Task 21.1:** Implement **video stabilization** (compute shader or CPU fallback).
- [ ] **Task 21.2:** Implement **motion tracking** (track an object across frames).
- [ ] **Task 21.3:** Implement **object detection** overlays (bounding boxes per frame).

---

## High-Speed Engineering Standards (Rules for AI Agents)
1. **Zero Allocations in Render Loop:** Pre-allocate all textures and buffers.
2. **Zero-Copy Pipeline:** Pixels stay in VRAM. Never convert `GstSample` to `Vec<u8>` on the CPU.
3. **No Global Locks:** Communicate using async channels (`tokio::sync::mpsc`).
4. **Hardware Affinity:** Always request `HighPerformance` Discrete GPU during adapter selection.
