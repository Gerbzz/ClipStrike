# Plan: Media Management Plus (Track 005)

## Phase 1: Native Drag-and-Drop
- [x] **Task 1.1:** Implement `egui::ViewportEvent::DroppedFile` listener in `ClipStrikeApp::update`.
- [x] **Task 1.2:** Integrate dropped file handling with `Project::add_asset` logic.
- [x] **Task 1.3:** Verify drag-and-drop support with a Playwright test (simulating drops).

## Phase 2: Async Thumbnail Engine
- [x] **Task 2.1:** Create a `ThumbnailGenerator` in `crates/clipstrike-media/src/probe.rs`.
- [x] **Task 2.2:** Setup a `tokio` mpsc channel for thumbnail requests/responses in `ClipStrikeApp`.
- [x] **Task 2.3:** Use GStreamer to extract frames at 10% duration without initializing the full engine.

## Phase 3: Visual Grid & Relinking
- [x] **Task 3.1:** Update `Media Browser` to display assets in a grid with thumbnails.
- [x] **Task 3.2:** Implement project-wide asset verification on startup.
- [x] **Task 3.3:** Build the "Missing Media" UI overlay and "Relink" context menu.

## Verification
- [ ] Run `cargo check --all-targets`.
- [ ] Manual test: Drag an mp4 from Windows Explorer into the Media Browser.
- [ ] Manual test: Rename an asset file on disk and verify the relinker detects it.
