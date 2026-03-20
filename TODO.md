# ClipStrike Backlog

This is the remaining work only. Completed history lives in `conductor/tracks.md`.

## Beta Release Core (Track 010)
- [x] Task 1.1: Add export destination picker and persist the last export directory.
- [x] Task 1.2: Wire the export job orchestration with frame request/response channels.
- [x] Task 1.3: Surface export completion, failure, and cancel state in the UI.
- [x] Task 2.1: Add a save/reopen smoke test for project serialization integrity.
- [x] Task 2.2: Add a playback/scrub smoke test for the main editor loop.
- [x] Task 2.3: Add a beta-safe feature gate or UI guard for incomplete AI/Vision controls.
- [x] Verification: Run `cargo check --all-targets`.
- [x] Verification: Manual test export a project and open the output file.
- [x] Verification: Manual test save, reopen, and confirm timeline state is preserved.

## Active Track 009: AI & Vision Features
- [x] Task 3.1: Implement object detection on a single frame (Preroll).
- [x] Task 3.2: Add "Smart Labels" to the UI over detected objects.
- [x] Task 3.3: Implement "Auto-Face-Blur" effect using detection results.
- [x] Verification: Manually track a moving object in a video and verify keyframes are generated.
- [x] Verification: Manually verify detection labels appear correctly in the preview.

## Engineering Rules
- [x] Keep the render loop allocation-free.
- [x] Keep the media path zero-copy.
- [x] Avoid global locks.
- [x] Use hardware-affine decoding and rendering.
- [x] Use `tokio` channels for async messaging.
- [x] Wire preview transform matrices through the FX path for clip motion and stabilization.












## UI/UX Polish (Track 011)
- [x] Task 4.1: Add a deterministic recovery checkpoint path helper.
- [x] Task 4.2: Wire autosave checkpoints into the native editor loop.
- [x] Task 4.3: Add a recovery banner/prompt and native smoke coverage.
- [x] Verification: Run cargo check --all-targets.
- [x] Task 4.5: Add right-click marker creation on the time ruler.






- [x] Task 4.4: Add an external-backed preview bypass for neutral clips.


## Conductor Mission Progress
- [x] Phase 1: Video preview plays clips without crashing.
- [x] Phase 1: Scrubbing timeline does not crash.
- [x] Phase 1: App stays running for a full session.
- [x] Preview audio decode now safely handles misaligned GStreamer buffers and no longer panics in the clip entry path.
- [x] Phase 2: Move clips between tracks.
- [x] Phase 3: Play/pause with spacebar.
- [x] Proxy generation now creates real output files and downsamples image sources.
- [x] Imported media clips now preserve audio-enabled state and stream live audio samples instead of forcing empty preloads.
- [x] Preview-engine crash fixed by removing the `playbin3` flags mutation that panicked on the wrong GLib value type. Verified with `cargo test -p clipstrike-media --features gstreamer-backend` and `cargo check -p clipstrike-app --features gstreamer-backend`.
- [x] Razor/Slice actions now dispatch the undoable `RazorClipCommand` instead of mutating the timeline directly.
- [x] Split Audio now picks a real neighboring track id instead of targeting a missing track, so the detached audio clip is visible in the timeline.
- [x] Playback no longer forces an audio seek every frame; explicit scrubs and transport jumps now resync audio instead of clearing streaming buffers continuously.
- [x] Audio streaming buffers now drain FIFO with `VecDeque` instead of `Vec::remove(0)`, which removes the main playback stutter source in the real-time audio callback.
- [x] Playback now prewarms the next clip on each track before the playhead reaches it, which reduces decoder startup stalls at clip boundaries.
- [x] Timeline clips now show frame thumbnails when the decoder has a frame texture, instead of relying only on flat fill colors.
- [x] Timeline markers now render as flag-style ruler handles with right-click edit/delete/color controls, which avoids the old overlapping label clutter.
- [x] Native preview sink setup now catches hardware-acceleration panics and falls back to a direct appsink path, which stops the startup crash when the first clip becomes active.
- [x] Export presets now drive the export pipeline's container and encoder selection, with hardware/software encoder fallback chosen by the GStreamer backend.
- [x] Verification: `cargo check --all-targets`, `cargo check -p clipstrike-app --features gstreamer-backend`, `cargo test -p clipstrike-ui --lib`.
- [x] Verification: `cargo test -p clipstrike-audio`, `cargo test -p clipstrike-ui --lib`, `cargo test -p clipstrike-core`, `cargo check -p clipstrike-ui`, `cargo check -p clipstrike-app --features gstreamer-backend`.
