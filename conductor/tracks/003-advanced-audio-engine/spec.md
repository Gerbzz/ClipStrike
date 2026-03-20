# Spec: Advanced Audio Engine (Track 003)

## Overview
Implement a robust, real-time audio playback and mixing engine that matches OpenShot's capabilities, using `cpal` for native output and synchronized with the video timeline.

## Requirements
- **Native Audio Output:** Integrate `cpal` to handle real-time audio playback on Windows (ASIO/WASAPI).
- **Synchronized Mixing:** 
    - Mix multiple audio streams from different tracks based on the current playhead position.
    - Support volume and panning keyframes (Phase 9 integration).
- **Audio Extraction (Split Audio):** Extract the audio stream from a video file and create a separate timeline clip.
- **Waveform Rendering:** Display accurate, multi-channel waveforms on clips in the timeline.
- **Master Volume:** Provide a global master volume slider in the UI.
- **Audio Effects:** Support basic effects (Fade In/Out, Compressor, Delay) via DSP or GStreamer plugins.

## Success Criteria
- [ ] Real-time audio plays back in sync with the video preview.
- [ ] Multiple clips on different tracks mix their audio correctly.
- [ ] Volume keyframes accurately change the loudness of a clip over time.
- [ ] Waveforms are visible and update when a clip is trimmed.
