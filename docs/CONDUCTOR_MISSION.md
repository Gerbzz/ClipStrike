# Conductor Mission: ClipStrike

## Goal
Achieve full OpenShot feature parity. Do not stop until every feature works.
Current blocker: app is sluggish when moving playhead between clips when the clip is playing in the preview window. Also there is a lot of stuttering when the clip is playing in the preview window. The clips have no audio as well when the playhead is moving through the timeline but the actual clip has audio. The Slice tool does not work well, it just creates a new clip at the playhead position but does not split the clip at the playhead position. I also cant seem to drag the clips to extend them or shrink them etc, I should be able to drag the edges of the clips to extend them or shrink them like a professinoal editor would have. The split audio tool does not work either, it just does nothing as i can tell on the ui doesn't change. The Marker tool works but the text overlaps eachother and id rather have a flag or somthing  attached to a line, then if a user right clicks on it can add a label or delete it. change color, etc. I also want the clip to not be just a solid color, it should be a preview of the clip itself in the timeline, and the audio should have a waveform preview as well. in additon the exporting is taking way to long, it should be much faster than this. MAke sure audit of the logs as well, and make sure you are using the hardware acceleration to its full potential. i want clean logs, if theres warnings in the logs i want you to review them and fix them and document them in the logs. in the timeline when a clip is 80percent done being played, towards the end, the video preview will freeze for a second, and then the red line will jump to the other clip and start playing that one. it skips the last 20 percent of the timeline clip but the clip is already done playing. at 80 percent, and there is still 20 percent left of the clip to play, the video preview will freeze for a second, and then the red line will jump to the other clip and start playing that one. it skips the last 20 percent of the timeline clip but the clip is already done playing.

# 🎬 Full Video Editor – Expected Behavior Specification (OpenShot Parity)

---

## 1. Core Editing Model (Non-Linear Editing)

### Expected Behavior
- The editor is **non-destructive**
- Original media files are never modified
- Timeline is a **composition of references** to source media

### Rules
- Each clip is defined as:
  (source_media, in_point, out_point, timeline_position, track_index)
- Multiple clips can reference the same media without duplication
- All actions must be undoable and redoable
- No destructive edits to source files under any circumstance

---

## 2. Timeline System

### Expected Behavior
- Supports multiple tracks (video + audio)
- Clips can be placed freely along a continuous time axis
- Timeline is zoomable (time scale adjusts visually and functionally)

### Interactions
- Drag clip → move along timeline
- Drag clip between tracks → change track
- Drag edges → trim clip
- Drag over another clip → snapping or overlap rules apply

### Snapping
- Clips snap to:
  - playhead
  - clip edges
  - markers
- Snapping should feel responsive and optional (toggleable)

---

## 3. Playback Engine

### Expected Behavior
- Smooth real-time playback (target 30–60 FPS)
- Playback reflects full timeline composition:
  - layering
  - effects
  - transitions

### Behavior While Playing
- Moving playhead does NOT freeze UI
- Playback pipeline should not fully restart unnecessarily
- Playback should remain responsive even under load

### Rules
- Frame dropping is acceptable under load
- Audio must remain in sync even if frames drop

---

## 4. Playhead / Scrubbing

### Expected Behavior
- Playhead can be:
  - clicked to reposition
  - dragged freely
  - stepped frame-by-frame via keyboard

### Scrubbing Behavior
- Visual updates must be immediate
- Acceptable:
  - muted audio during fast scrubbing
- Not acceptable:
  - UI lag
  - broken playback after scrubbing
  - desynced audio/video

---

## 5. Clips

### Expected Behavior
Each clip must support:
- Move (drag)
- Trim (resize edges)
- Split (cut at playhead)
- Duplicate
- Delete

### Trim Behavior
- Left edge drag → adjusts in_point
- Right edge drag → adjusts out_point
- Cannot exceed source media bounds

### Split Behavior
- Splitting creates two clips:
  - Clip A: start → playhead
  - Clip B: playhead → end
- Both retain reference to same source media
- Must preserve timing and sync

---

## 6. Tracks

### Expected Behavior
- Unlimited tracks (within performance constraints)
- Two primary types:
  - Video tracks
  - Audio tracks

### Layering Rules
- Higher tracks render above lower tracks
- Audio tracks mix together

### Track Controls
- Mute
- Solo
- Lock (prevents editing)

---

## 7. Audio System

### Expected Behavior
- Audio plays in sync with video at all times
- Each clip supports:
  - volume control
  - mute toggle
  - detaching (split audio)

### Split Audio
- Separates audio into its own clip
- Audio appears on audio track
- Video clip becomes independent

### Waveform Rendering
- Audio clips display waveform
- Generated asynchronously
- Cached for performance

---

## 8. Video Preview Rendering

### Expected Behavior
- Preview accurately reflects timeline state
- Includes:
  - transforms
  - effects
  - transitions
  - layering

### Performance Rules
- Use GPU rendering where possible
- Avoid full re-decode when frame can be reused
- Use frame caching to reduce stutter

---

## 9. Transitions

### Expected Behavior
- Applied between overlapping clips
- Common transitions:
  - fade
  - dissolve
  - wipe

### Behavior
- Transition duration adjustable
- Smooth interpolation between clips
- Visual preview updates in real time

---

## 10. Effects System

### Expected Behavior
- Effects applied per clip
- Effects are stackable
- Order of effects matters

### Examples
- brightness / contrast
- blur
- color grading
- transform (scale, rotate, position)

### Rules
- Real-time preview when possible
- GPU acceleration preferred
- No blocking UI during effect processing

---

## 11. Markers

### Expected Behavior
- Markers attach to timeline positions
- Displayed as:
  - vertical line + flag/icon

### Interactions
- Right-click options:
  - rename
  - delete
  - change color

### Rules
- No overlapping text clutter
- Scales cleanly with zoom level

---

## 12. Timeline Visuals

### Expected Behavior

#### Video Clips
- Display thumbnail strip (filmstrip style)
- Thumbnail density adjusts with zoom level

#### Audio Clips
- Display waveform visualization
- Scales based on zoom

### Performance
- Generated asynchronously
- Cached to avoid recomputation
- Must not block UI

---

## 13. Selection System

### Expected Behavior
- Click → select single clip
- Shift-click → multi-select
- Drag box → area select

### Rules
- Selected clips visibly highlighted
- All actions apply to current selection

---

## 14. Undo / Redo System

### Expected Behavior
- Every action is reversible:
  - move
  - trim
  - split
  - delete
  - effect changes

### Rules
- Stack-based history system
- No data corruption after repeated undo/redo cycles

---

## 15. Export System

### Expected Behavior
- Renders final video based on timeline composition
- Supports:
  - resolution selection
  - format selection (mp4, etc.)
  - bitrate control

### Performance
- Uses hardware encoding when available (GPU)
- Faster than real-time when possible

### Rules
- Output must match preview (WYSIWYG)
- Logs must indicate:
  - encoder used
  - GPU vs CPU usage
  - fallback reasons if GPU not used

---

## 16. Performance Requirements

### Expected Behavior
- UI must never freeze
- Timeline interactions must feel instant
- Playback must degrade gracefully (no crashes)

### Strategies
- Frame caching
- Background decoding threads
- GPU rendering pipeline
- Avoid main-thread blocking

---

## 17. Logging / Debugging

### Expected Behavior
Logs must include:
- playback state transitions
- dropped frames
- decode/encode pipeline usage
- GPU vs CPU usage
- warnings with actionable context

### Rules
- No log spam
- No unexplained warnings
- Errors must include clear diagnostic information

---

## 18. Hardware Acceleration

### Expected Behavior
- GPU used for:
  - preview rendering
  - effects processing
  - encoding/export

### Rules
- If GPU is not used → log the reason clearly
- Avoid unnecessary CPU ↔ GPU memory transfers
- Ensure pipeline is truly GPU-accelerated (not partially CPU-bound)

---

## ✅ Final Acceptance Criteria

The editor meets expectations only when:

1. Playback is smooth and responsive
2. Timeline interactions do not lag
3. Clips can be moved, trimmed, and split correctly
4. Audio behaves correctly and stays in sync
5. Split Audio visibly works
6. Markers are clean and usable
7. Timeline shows video thumbnails and audio waveforms
8. Export is fast and uses hardware acceleration
9. Logs are clean, meaningful, and free of unresolved warnings
10. No major UI or playback stuttering under normal usage

---

## Current Crash
The app crashes when the playhead reaches a clip in the timeline and the video
preview attempts to display and play that clip. Fix this first, then continue
with full feature parity work.

## How To Work
- Read docs/LOCAL_SETUP.md for GStreamer/MSVC paths before touching media code
- If a package or dependency is missing, install it and keep going
- If a build fails, fix it and keep going
- If a test fails, fix the code and keep going
- Never stop to ask for confirmation
- Never stop to report and wait for a response
- After fixing the crash, keep going through the feature list below
- Update TODO.md after completing each feature so progress survives session resets

## Architecture Rules (do not violate these)
- Timeline logic belongs in clipstrike-core, not clipstrike-ui
- GStreamer stays in clipstrike-media
- GPU code stays in clipstrike-render
- clipstrike-app stays thin - wiring only
- Add tests to clipstrike-core before wiring new editor gestures

## How To Verify Each Fix
- Crash fix: run `run_clipstrike_gstreamer.cmd`, scrub playhead over a clip,
  confirm video preview plays without crash for 30+ seconds
- Tests: `cargo test -p clipstrike-core` must pass
- Build: `cargo check -p clipstrike-app --features gstreamer-backend` must pass
- Do not mark anything fixed unless the actual verification command passed

## Full Feature Parity List (work through in order)
### Phase 1 - Stability (do first)
- [ ] Video preview plays clips without crashing
- [ ] Scrubbing timeline does not crash
- [ ] App stays running for a full session

### Phase 2 - Core Timeline
- [ ] Drag clips onto timeline
- [ ] Trim clip start/end on timeline
- [ ] Snap clips to each other and to playhead
- [ ] Multi-track support (video + audio tracks)
- [ ] Undo/redo for all timeline operations
- [ ] Cut/split clip at playhead
- [ ] Move clips between tracks

### Phase 3 - Playback
- [ ] Play/pause with spacebar
- [ ] Scrub preview while dragging playhead
- [ ] Loop playback region
- [ ] Audio plays in sync with video preview
- [ ] Frame-accurate seek

### Phase 4 - Media
- [ ] Import video files (mp4, mov, mkv, webm)
- [ ] Import audio files (mp3, wav, aac)
- [ ] Import image files (png, jpg)
- [ ] Proxy generation for large files
- [ ] Waveform display on audio clips

### Phase 5 - Effects & Transitions
- [ ] Fade in/out on clips
- [ ] Cross-dissolve transition between clips
- [ ] Keyframe editor for clip properties
- [ ] Basic color correction (brightness, contrast, saturation)
- [ ] Speed change on clips

### Phase 6 - Export
- [ ] Export timeline to mp4
- [ ] Export with audio
- [ ] Progress bar during export
- [ ] Cancel export

### Phase 7 - Project
- [ ] Save project to file
- [ ] Load project from file
- [ ] Recent projects list
- [ ] Auto-save

## Definition of Done
Every item in the feature list above is checked off, all tests pass, and the app
runs a full edit-and-export session without crashing.