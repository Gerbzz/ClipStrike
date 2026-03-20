# Plan: AI & Vision Features (Track 009)

## Phase 1: Infrastructure & Data Models
- [x] **Task 1.1:** Add `opencv` dependency to `clipstrike-media` (gate with feature).
- [x] **Task 1.2:** Add `MotionTrack` model to `crates/clipstrike-core/src/effects.rs`.
- [x] **Task 1.3:** Setup an async worker in `ClipStrikeApp` for analysis tasks.

## Phase 2: Motion Tracking (CSRT/KCF)
- [x] **Task 2.1:** Implement a basic tracker in `clipstrike-media` that processes a sequence of frames.
- [x] **Task 2.2:** Add "Start Tracking" logic to the UI that spawns the analysis task.
- [x] **Task 2.3:** Map tracking results back to `pos_x` and `pos_y` keyframes.

## Phase 3: Object Detection (YOLO/SSD)
- [x] **Task 3.1:** Implement object detection on a single frame (Preroll).
- [x] **Task 3.2:** Add "Smart Labels" to the UI over detected objects.
- [x] **Task 3.3:** Implement "Auto-Face-Blur" effect using detection results.

## Verification
- [x] Run `cargo check --all-targets`.
- [x] Manual test: Track a moving object in a video and verify keyframes are generated.
- [x] Manual test: Verify detection labels appear correctly in the preview.

