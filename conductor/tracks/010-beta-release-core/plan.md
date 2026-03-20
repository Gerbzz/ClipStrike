# Plan: Beta Release Core (Track 010)

## Phase 1: Export Path
- [x] Task 1.1: Add export destination picker and persist the last export directory.
- [x] Task 1.2: Wire the export job orchestration with frame request/response channels.
- [x] Task 1.3: Surface export completion, failure, and cancel state in the UI.

## Phase 2: Beta Smoke Coverage
- [x] Task 2.1: Add a save/reopen smoke test for project serialization integrity.
- [x] Task 2.2: Add a playback/scrub smoke test for the main editor loop.
- [x] Task 2.3: Add a beta-safe feature gate or UI guard for incomplete AI/Vision controls.

## Verification
- [ ] Run `cargo check --all-targets`.
- [x] Manual test: export a project and open the output file.
- [x] Manual test: save, reopen, and confirm the timeline state is preserved.






