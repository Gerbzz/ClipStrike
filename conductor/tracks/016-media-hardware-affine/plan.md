# Media Hardware-Affine Preview Plan

**Objective:** Make the native preview sink negotiate GL-backed samples first so external texture metadata can flow through the media path without forcing an unnecessary CPU download.

## Step 1
- [x] Add a helper for video sink caps that can request `memory:GLMemory`.
- [x] Add a focused unit test that proves the caps helper exposes the GLMemory feature set when requested.

## Step 2
- [x] Use the helper in the native GStreamer engine.
- [x] Keep the CPU download branch intact as a fallback path so launch behavior stays stable.

## Step 3
- [x] Run the focused GStreamer backend tests.
- [x] Re-run `cargo check --all-targets`.
- [x] Update conductor status and implementation state notes.
