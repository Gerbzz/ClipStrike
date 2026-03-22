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
- [x] Blocker 1 verified. Timeline duration now requires fully decoded duration instead of discoverer/container fallback, and `cargo test -p clipstrike-ui windows_playback_boundary_smoke_stops_at_clip_end -- --ignored --nocapture` confirms playback stops at clip end.
- [x] Blocker 2 verified. Interactive scrub seeks now avoid GStreamer flush seeks during playback; verified with `cargo test -p clipstrike-media interactive_seek_avoids_flush_but_keeps_keyframe_alignment --features gstreamer-backend` and `cargo test -p clipstrike-ui scrub_playhead_clamps_target_to_timeline_end --lib`.
- [x] Blocker 3 verified. Playback engine state changes are now delta-based so active clips are not re-issued `play` every frame; verified with `cargo test -p clipstrike-media playback_state_deltas --lib`.
- [x] Blocker 4 verified. Paused scrubs now trigger a short audio preview burst; verified with `cargo test -p clipstrike-audio scrub_preview --lib`.
- [x] Blocker 5 verified. Razor clicks now convert clip-local pointer positions into absolute timeline split times before dispatching the split command; verified with `cargo test -p clipstrike-ui clip_split_time_from_pointer_offsets_by_clip_start --lib`.
- [x] Blocker 6 verified. Edge trims now clamp against the source asset duration instead of allowing the clip to extend past media bounds; verified with `cargo test -p clipstrike-core preview_trim_clamps_and_snaps_only_the_dragged_edge --lib`, `cargo test -p clipstrike-core left_trim_cannot_extend_past_source_duration --lib`, and `cargo test -p clipstrike-core right_trim_cannot_extend_past_source_duration --lib`.
- [x] Blocker 11 verified. Removed the Project Files placeholder `+` path that inserted `asset_id=0` clips, materialized title clips through real assets, skipped zero-duration engine sync, and filtered non-actionable `wgpu_hal` warning spam; verified with `cargo test -p clipstrike-ui windows_playback_boundary_smoke_stops_at_clip_end --test native_smoke -- --ignored --nocapture`, `rg -n "WARN|ERROR" tmp/playback_smoke_capture.log` (no matches), and `cargo check -p clipstrike-app --features gstreamer-backend` using the repo-local GStreamer runtime at `gst_extracted/PFiles64/gstreamer/1.0/msvc_x86_64`.
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
- [x] Playhead sync fix: probe duration now falls back to discoverer duration when decode probe fails; discrete seeks now use ACCURATE instead of KEY_UNIT for frame accuracy; playback loop now derives playhead from GStreamer's actual position via `query_position_ms()` → `timeline_time_from_source()` instead of free-running with UI frame delta, eliminating clock drift. Verified with `cargo test -p clipstrike-core`, `cargo check -p clipstrike-app --features gstreamer-backend`.
- [x] Playback sluggishness fix: time ruler drag interactions now pause playback via `engine_manager.pause_all()` on drag start, and resume active clips on drag release. Frame alignments use a cached `last_track_content_origin_x` across UI passes. Verified with `cargo test -p clipstrike-ui --lib` and `cargo check -p clipstrike-app --features gstreamer-backend`.
