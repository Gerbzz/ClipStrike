# Plan: Advanced Audio Engine (Track 003)

## Phase 1: cpal Integration & Playback
- [x] **Task 1.1:** Setup `cpal` device initialization in `crates/clipstrike-audio/src/lib.rs`.
- [x] **Task 1.2:** Implement a ring-buffer based sample provider for low-latency playback.
- [x] **Task 1.3:** Connect the GStreamer audio appsink to the `cpal` output stream.

## Phase 2: Mixing & Keyframes
- [x] **Task 2.1:** Implement a multi-track mixer that sums samples from all active clips.
- [x] **Task 2.2:** Integrate volume keyframe evaluation into the mixing loop.
- [x] **Task 2.3:** Add global master volume control.

## Phase 3: Split Audio & UI
- [x] **Task 3.1:** Implement "Split Audio" command to extract audio tracks as new clips.
- [x] **Task 3.2:** Optimize waveform generation using background threads.
- [x] **Task 3.3:** Add audio level meters to the UI.

## Verification
- [ ] Run `cargo check --all-targets`.
- [ ] Manual test: Verify audio plays smoothly without crackling.
- [ ] Manual test: Verify audio stays in sync with video over 1 minute of playback.
