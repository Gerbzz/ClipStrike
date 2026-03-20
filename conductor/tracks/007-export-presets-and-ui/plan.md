# Plan: Export Presets & UI (Track 007)

## Phase 1: Preset Model & Configuration
- [x] **Task 1.1:** Define `ExportPreset` struct in `crates/clipstrike-media/src/export.rs`.
- [x] **Task 1.2:** Add a default list of presets to `AppConfig`.
- [x] **Task 1.3:** Create a basic "Export" dialog in the UI allowing selection of these presets.

## Phase 2: Async Progress Tracking
- [x] **Task 2.1:** Implement progress reporting in the GStreamer export pipeline.
- [x] **Task 2.2:** Add `export_progress_rx` channel to `ClipStrikeApp` to receive real-time updates.
- [x] **Task 2.3:** Implement the `ExportProgress` modal dialog with a percentage bar.

## Phase 3: UX Polish & Robustness
- [ ] **Task 3.1:** Implement "Cancel Export" functionality.
- [ ] **Task 3.2:** Add "Estimated Time Remaining" calculation.
- [ ] **Task 3.3:** Implement a success/failure notification after export completes.

## Verification
- [ ] Run `cargo check --all-targets`.
- [ ] Manual test: Export a 30-second clip using the YouTube preset and verify the progress bar.
- [ ] Manual test: Cancel an export halfway and verify the file is cleaned up.
