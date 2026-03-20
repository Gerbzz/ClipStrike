# Specification: Media Management Plus (Track 005)

## Overview
Enhance ClipStrike's media handling capabilities by integrating native OS drag-and-drop, generating automated thumbnails for assets, and providing a robust relinking system for missing media files.

## Goals
- **Native Interop:** Support `egui`'s dropped files API to import media from the OS file explorer.
- **Visual Feedback:** Replace generic icons with frame-accurate thumbnails for video assets in the Media Browser.
- **Project Reliability:** Detect broken asset links and provide a "Relink" dialog.

## Requirements

### 1. OS Drag-and-Drop
- Listen for `egui::ViewportEvent::DroppedFile` events.
- Validate file extensions (supported formats: mp4, mkv, mov, avi, mp3, wav, png, jpg).
- Automatically add valid files to the `Project` assets and `AssetCache`.

### 2. Thumbnail Generation
- Use GStreamer's `preroll` capability to capture a frame at the 10% mark of video duration.
- Convert raw video frames to `egui::ColorImage`.
- Store thumbnails in `AssetCache` and render them in the Media Browser grid.

### 3. Asset Relinking
- On project load, verify the existence of all `MediaAsset.uri` paths.
- Mark missing assets with a visual "Missing" overlay in the UI.
- Implement a `Relink` context menu option that opens a file picker to update the URI.

## Technical Architecture
- **Media Pipeline:** Extend `clipstrike-media` to include a `ThumbnailGenerator` that runs outside the main playback loop.
- **Async Messaging:** Use `tokio` channels to send thumbnail generation requests and receive processed images.
- **UI:** Update `Media Browser` in `clipstrike-ui` to support grid view with thumbnails and drop targets.
