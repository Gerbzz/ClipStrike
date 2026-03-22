# Conductor Mission: ClipStrike

## Goal

Achieve full OpenShot feature parity. Do not stop until every feature works.

---

## Current Blockers (Fix in Order)

1. **Playhead overshoots clip end / stutter at clip boundary** — When a clip finishes playing, the playhead continues advancing past the clip's actual end into a small region of dead time (e.g. a 5-second clip plays to 5.0s but the timeline still has ~0.5s of nothing left). The video preview freezes during this dead region, then the playhead skips forward to the real end of the clip. This is caused by a duration mismatch between what the timeline stores and what the media actually contains. Fix requirements:
   - Clip duration stored on the timeline must be derived from the fully decoded media duration after probe completes — never from container metadata estimates.
   - `out_point` must be clamped to the real decoded duration of the source file at probe time.
   - When the decoder signals EOS for a clip, the playhead must snap immediately to `clip.timeline_end` and transition cleanly to the next clip or stop. The playhead must never advance past `clip.timeline_end`.
   - If timeline ticks are frame-count-based, ensure FPS assumptions match the source media exactly to avoid accumulated rounding error at clip boundaries.

2. **Playback sluggishness** — Moving the playhead while a clip is playing in the preview window is slow and unresponsive.

3. **Playback stuttering** — Video preview stutters during playback.

4. **No audio during scrubbing** — Clips have audio, but no audio plays while the playhead moves through the timeline.

5. **Slice tool broken** — Creates a new clip at the playhead position instead of splitting the existing clip at that point into two clips (Clip A: start → playhead, Clip B: playhead → end).

6. **Clip trimming broken** — Dragging clip edges to extend or shrink clips does not work. Left edge drag must adjust in-point; right edge drag must adjust out-point. Neither edge may exceed source media bounds.

7. **Split Audio does nothing** — The split audio function has no visible effect on the UI. It must separate the audio track from the clip and place it independently on an audio track.

8. **Marker text overlap** — Markers display overlapping text. Replace with a flag on a vertical line. Right-clicking a marker must offer: rename, delete, change color.

9. **Clip visuals** — Video clips must display a filmstrip thumbnail preview, not a solid color. Audio clips must display a waveform.

10. **Export speed** — Export is too slow. Hardware acceleration must be used. Logs must confirm which encoder and GPU path was used, and must explain any fallback to CPU.

11. **Log cleanliness** — Audit all logs. Resolve all warnings. Document fixes. No unexplained warnings should remain.

12. **Media Drag and Drop** — Drag and drop media files onto the timeline, should not be dragging an invisible object. It should be dragging the media file itself. Right now it is invisible until you release the mouse button on the timeline. It should be visible the entire time you are dragging it. It should also be transparent so you can see where you are dragging it to.

13. **Timeline readability** — The timeline is currently very hard to read. The clips are too small height wise. The clips should be larger and more visible. The timeline should also be zoomable so that the user can see more or less of the timeline at once. I would also like the clips to be color coded based on the type of media it is. Video clips a certain color, audio clips a certain color, and image clips a certain color, and so on. I Would also like when audio is split from a video clip, that the audio clip is placed on a separate audio track below the video clip but they are grouped together so that if you move the video clip, the audio clip moves with it and vice versa. until you right click and select "ungroup" or something similar. I think there should be a border color around every new clip added that is random, but shares the same color if you split the audio, so that you can easily see which audio belongs to which video clip.

14. **Timeline zoom** — The timeline should be zoomable so that the user can see more or less of the timeline at once.

15. **Timeline Playhead When adding more tracks** — When adding more tracks to the timeline, the playhead should increase in size to match the new track height or however you decide to handle it. It should be the same size as the clips on the timeline. Right now its not increasing in size when new tracks are added.

16. **Export Clip** — When the user right clicks on a clip, there should be an option to "Export Clip". This should export the clip to a file, similar to the "Export Project" option but only for the selected clip. It should use the same export settings as the project, but only for the selected clip. It should also have its own set of export settings that the user can change. It should also have a progress bar that shows the progress of the export. It should also have a progress bar that shows the progress of the export.

17. **Export** — is slow. It should be faster. It should use hardware acceleration. It should also have a progress bar that shows the progress of the export. It should also have a progress bar that shows the progress of the export.

18. **Remove Track button in timeline** — Should have a option for are you sure you want to remove this track? 

19. **Double audio waveforms** — Right now theres 2 audio waveforms in the video clip and when you seperate the audio you get another below and the 2 above stay there. It should be just one audio waveform in the video clip and when you seperate the audio you get another below and the 2 above should disappear.

20. **Split Audio** — When you split the audio from a video clip, the audio clip should be placed on a separate audio track below the video clip but they should be grouped together so that if you move the video clip, the audio clip moves with it and vice versa. until you right click and select "ungroup" or something similar. I think there should be a border color around every new clip added that is random, but shares the same color if you split the audio, so that you can easily see which audio belongs to which video clip. Right now when moving the video clip with audio below in its own track the other grouped clip is not moving until you let go of the mouuse button, it would be nice if they moved in real time as you drag the video clip. etc.

21. **laggy preview** — The preview is laggy. It should be smooth. It should use hardware acceleration. It should also have a progress bar that shows the progress of the preview. It should also have a progress bar that shows the progress of the preview. When you drag a clip to the timeline it should be transparent so you can see where you are dragging it to. It should also be visible the entire time you are dragging it. Right now it is invisible until you let go of the mouse button.

22. **avoid duplicate media items** — when importing media right now i can import the same media file multiple times. It should not allow you to import the same media file multiple times. It should warn you if you are trying to import the same media file multiple times. It should also have a option to remove duplicate media items. 

23. **Write tests for all features** — Focus on optimization and performance. You should create tests on speed performance with current code, and try to improve it. The tests should write Current speed : time it took to complete. Optimized speed : time it took to complete. And if its worth it we should implement the optimized code. Use conductor to help you write the specs and plan. The goal is to make the app as fast and efficient as possible. 

---

## Architecture Rules (Do Not Violate)

- Timeline logic → `clipstrike-core`
- GStreamer → `clipstrike-media`
- GPU code → `clipstrike-render`
- `clipstrike-app` stays thin — wiring only
- Add tests to `clipstrike-core` before wiring new editor gestures

---

## How To Work

- Read `docs/LOCAL_SETUP.md` for GStreamer/MSVC paths before touching media code
- If a package or dependency is missing, install it and keep going
- If a build fails, fix it and keep going
- If a test fails, fix the code and keep going
- Never stop to ask for confirmation
- Never stop to report and wait for a response
- Update `TODO.md` after completing each feature so progress survives session resets

---

## How To Verify Each Fix

| Fix | Verification |
|-----|-------------|
| Clip boundary / EOS fix | Place a clip on the timeline, play it to the end, confirm the playhead stops exactly at `clip.timeline_end` with no freeze, stutter, or skip. Repeat with clips of varying durations and frame rates. |
| Crash fix | Run `run_clipstrike_gstreamer.cmd`, scrub playhead over a clip, confirm video preview plays without crash for 30+ seconds |
| Tests | `cargo test -p clipstrike-core` must pass |
| Build | `cargo check -p clipstrike-app --features gstreamer-backend` must pass |

Do not mark anything fixed unless the actual verification command passed.

---

## Full Feature Parity List

### Phase 1 — Stability (Do First)
- [ ] Video preview plays clips without crashing
- [ ] Scrubbing timeline does not crash
- [ ] App stays running for a full session
- [ ] Playhead stops exactly at clip end with no stutter or skip

### Phase 2 — Core Timeline
- [ ] Drag clips onto timeline
- [ ] Trim clip start/end by dragging edges
- [ ] Snap clips to each other and to playhead
- [ ] Multi-track support (video + audio tracks)
- [ ] Undo/redo for all timeline operations
- [ ] Cut/split clip at playhead
- [ ] Move clips between tracks

### Phase 3 — Playback
- [ ] Play/pause with spacebar
- [ ] Scrub preview while dragging playhead
- [ ] Loop playback region
- [ ] Audio plays in sync with video preview
- [ ] Frame-accurate seek

### Phase 4 — Media
- [ ] Import video files (mp4, mov, mkv, webm)
- [ ] Import audio files (mp3, wav, aac)
- [ ] Import image files (png, jpg)
- [ ] Proxy generation for large files
- [ ] Waveform display on audio clips

### Phase 5 — Effects & Transitions
- [ ] Fade in/out on clips
- [ ] Cross-dissolve transition between clips
- [ ] Keyframe editor for clip properties
- [ ] Basic color correction (brightness, contrast, saturation)
- [ ] Speed change on clips

### Phase 6 — Export
- [ ] Export timeline to mp4
- [ ] Export with audio
- [ ] Progress bar during export
- [ ] Cancel export

### Phase 7 — Project
- [ ] Save project to file
- [ ] Load project from file
- [ ] Recent projects list
- [ ] Auto-save

---

## Expected Behavior Specification

### 1. Core Editing Model (Non-Linear)
- Non-destructive — original media files are never modified
- Timeline is a composition of references to source media
- Each clip is defined as: `(source_media, in_point, out_point, timeline_position, track_index)`
- Multiple clips may reference the same media without duplication
- All actions must be undoable and redoable

### 2. Timeline System
- Multiple video and audio tracks
- Clips placed freely along a continuous time axis
- Timeline is zoomable
- Drag clip → move along timeline
- Drag clip between tracks → change track
- Drag clip edges → trim
- Snapping to playhead, clip edges, and markers (toggleable)

### 3. Playback Engine
- Target: 30–60 FPS smooth playback
- Playback reflects full timeline composition (layering, effects, transitions)
- Moving the playhead must not freeze the UI
- Playback pipeline must not fully restart unnecessarily
- Frame dropping acceptable under load; audio must stay in sync

### 4. Playhead / Scrubbing
- Click to reposition; drag freely; step frame-by-frame via keyboard
- Visual updates must be immediate
- Audio may be muted during fast scrubbing
- UI lag, broken playback after scrubbing, and desynced audio/video are not acceptable

### 5. Clips
Each clip must support: move, trim, split, duplicate, delete.

**Trim:**
- Left edge drag → adjusts in-point
- Right edge drag → adjusts out-point
- Cannot exceed source media bounds

**Split:**
- Creates two clips: Clip A (start → playhead) and Clip B (playhead → end)
- Both reference same source media
- Timing and sync preserved

**Clip Boundary / EOS Behavior:**
- Clip duration is authoritative only after full media probe; never use container metadata estimates
- `out_point` is clamped to real decoded duration at probe time
- On EOS signal from decoder: playhead snaps to `clip.timeline_end` immediately
- Playhead never advances past `clip.timeline_end`
- Transition to next clip or stop must be immediate with no freeze or skip

### 6. Tracks
- Unlimited tracks (within performance constraints)
- Video tracks and audio tracks
- Higher tracks render above lower tracks; audio tracks mix together
- Per-track controls: mute, solo, lock

### 7. Audio System
- Audio plays in sync with video at all times
- Per-clip: volume control, mute toggle, detach (split audio)

**Split Audio:**
- Separates audio into its own clip on an audio track
- Video clip becomes independent

**Waveform:**
- Rendered asynchronously and cached

### 8. Video Preview Rendering
- Accurately reflects timeline state including transforms, effects, transitions, and layering
- GPU rendering used where possible
- Frame caching to reduce stutter
- Avoid full re-decode when frame can be reused

### 9. Transitions
- Applied between overlapping clips
- Types: fade, dissolve, wipe
- Duration adjustable; real-time preview

### 10. Effects System
- Applied per clip, stackable, order-sensitive
- Examples: brightness/contrast, blur, color grading, transform (scale, rotate, position)
- Real-time preview preferred; GPU acceleration required; no UI blocking

### 11. Markers
- Attach to timeline positions
- Displayed as a vertical line with a flag
- Right-click options: rename, delete, change color
- No overlapping text; scales with zoom level

### 12. Timeline Visuals

**Video clips:**
- Filmstrip thumbnail strip
- Thumbnail density adjusts with zoom

**Audio clips:**
- Waveform visualization
- Scales with zoom
- Generated asynchronously and cached; must not block UI

### 13. Selection System
- Click → select single clip
- Shift-click → multi-select
- Drag box → area select
- Selected clips visibly highlighted; all actions apply to selection

### 14. Undo / Redo
- Every action is reversible: move, trim, split, delete, effect changes
- Stack-based history; no data corruption after repeated undo/redo cycles

### 15. Export System
- Renders final video based on timeline composition
- Supports: resolution, format (mp4+), bitrate
- Uses hardware encoding (GPU) when available
- Output must match preview (WYSIWYG)
- Logs must indicate encoder used, GPU vs CPU, and fallback reasons if GPU not used

### 16. Performance Requirements
- UI must never freeze
- Timeline interactions must feel instant
- Playback degrades gracefully (no crashes)

Strategies: frame caching, background decoding threads, GPU rendering pipeline, no main-thread blocking.

### 17. Logging / Debugging
Logs must include:
- Playback state transitions
- Dropped frames
- Decode/encode pipeline usage
- GPU vs CPU usage
- Warnings with actionable context

Rules: no log spam, no unexplained warnings, errors include clear diagnostic info.

### 18. Hardware Acceleration
- GPU used for: preview rendering, effects, encoding/export
- If GPU not used → log the reason clearly
- Avoid unnecessary CPU ↔ GPU memory transfers
- Pipeline must be truly GPU-accelerated, not partially CPU-bound

---

## Definition of Done

Every item in the feature list is checked off, all tests pass, and the app completes a full edit-and-export session without crashing.
