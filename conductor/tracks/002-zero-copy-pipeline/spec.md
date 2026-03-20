# Spec: Zero-Copy Media Pipeline (Track 002)

## Overview
Initialize a high-performance media pipeline that decodes video frames using hardware acceleration and passes them directly to the GPU (wgpu) without CPU copies.

## Requirements
- **Hardware Decoding:** Use `playbin3` with a preference for `nvdec` (NVIDIA), `d3d11videosink` (Windows), or `vaapi` (Linux).
- **Zero-Copy Path:** 
    - Use `glupload` to ensure frames are uploaded to the GPU context.
    - Use `appsink` to intercept GStreamer samples.
    - Map GStreamer GL textures directly to `wgpu` External Textures or shared buffers.
- **Clock Synchronization:** Synchronize the GStreamer pipeline clock with the `wgpu` render loop to prevent jitter and ensure frame-accurate display.
- **Async Communication:** Use `tokio` channels to send frame availability signals from the GStreamer bus/appsink to the UI thread.

## Success Criteria
- [ ] GStreamer `playbin3` successfully initializes with a hardware decoder.
- [ ] Video frames are rendered in the ClipStrike preview window.
- [ ] Profiling shows minimal CPU usage during playback (indicating zero-copy is working).
- [ ] No allocations occur in the main `update` loop for frame display.
