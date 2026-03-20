# Specification: Export Presets & UI (Track 007)

## Overview
Enhance the export experience by providing standardized quality presets (YouTube, 4K, Mobile) and a dedicated UI for tracking export progress with real-time feedback.

## Goals
- **Simplified Export:** Allow users to choose from a list of common export profiles instead of manually configuring bitrate/encoders.
- **Progress Tracking:** Provide a modal dialog that shows rendering progress, estimated time remaining, and a "Cancel" option.
- **Background Rendering:** Ensure the UI remains responsive during export (leveraging existing GStreamer background pipelines).

## Requirements

### 1. Export Presets
- Define standard profiles:
    - **YouTube (1080p):** H.264, 30fps, 10Mbps bitrate.
    - **4K Ultra HD:** H.264/H.265, 60fps, 40Mbps bitrate.
    - **Mobile/Social:** H.264, 30fps, 5Mbps, Vertical aspect ratio support.
    - **Audio Only:** MP3/AAC, 192kbps.
- Store presets in `AppConfig` or a dedicated registry.

### 2. Export Progress UI
- Create an `ExportProgress` modal in `clipstrike-ui`.
- Show percentage complete based on GStreamer `position` / `duration` query.
- Add a "Cancel" button that stops the GStreamer pipeline and cleans up partial files.

### 3. File Handling
- Integration with `rfd` (Rust File Dialog) to pick the output filename.
- Graceful error handling for disk space or write permission issues.

## Technical Architecture
- **Media Pipeline:** Extend `clipstrike-media` export logic to accept a `Preset` struct.
- **Async Communication:** Use `tokio` watch or mpsc channels to stream progress from the export task to the UI.
- **State Management:** Track current export status (Idle, Exporting, Completed, Failed) in `ClipStrikeApp`.
