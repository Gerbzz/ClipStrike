# BLOCKER

The remaining open beta item, `Keep the media path zero-copy`, is blocked by a stack boundary in the current workspace:

- The native media pipeline can negotiate `memory:GLMemory` samples and preserve `external_texture_id` metadata.
- The current `wgpu`/`wgpu-types` version in this workspace exposes external-image copy types for browser/video-frame sources, but not a native GLMemory import surface that can adopt a GStreamer GL texture id directly on desktop.
- The UI/render path still requires a `wgpu::Texture` owned by the renderer, so the GLMemory sample still falls back to CPU readback if we need a visible native preview frame.

Verified commands and evidence:
- `rg -n "ExternalTexture|create_external|external texture" "$env:USERPROFILE\\.cargo\\registry\\src\\index.crates.io-1949cf8c6b5b557f\\wgpu-23.0.1" -g "*.rs"`
- `rg -n "ExternalTexture|create_external|external texture" "$env:USERPROFILE\\.cargo\\registry\\src\\index.crates.io-1949cf8c6b5b557f\\wgpu-types-23.0.0" -g "*.rs"`
- `cargo test -p clipstrike-media --features gstreamer-backend video_sink_caps_uses_glmemory_features_when_preferred -- --nocapture`
- `cargo test -p clipstrike-media --features gstreamer-backend frame_upload_request_from_buffer_preserves_external_metadata_and_pixels -- --nocapture`
- `cargo check --all-targets`

Current status:
- Native desktop playback, export, and smoke coverage are green.
- The zero-copy TODO remains open because the stack does not currently expose the native interop primitive needed to import a GStreamer GLMemory sample directly into the wgpu texture path.
