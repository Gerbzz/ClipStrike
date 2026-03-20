# Plan: Zero-Copy Media Pipeline (Track 002)

## Phase 1: GStreamer HW Initialization
- [x] **Task 1.1:** Update `crates/clipstrike-media/src/engine.rs` to use `playbin3`.
- [x] **Task 1.2:** Implement hardware decoder auto-discovery logic (preferring D3D11 on Windows).
- [x] **Task 1.3:** Setup the GStreamer bus to handle messages asynchronously via `tokio`.

## Phase 2: Zero-Copy wgpu Integration
- [x] **Task 2.1:** Configure `glupload` and `glcolorconvert` in the GStreamer bin.
- [x] **Task 2.2:** Implement an `appsink` callback that extracts the raw GPU pointer/handle from the `GstSample`.
    - [x] Extract `GLMemory` or `D3D11Memory` handles from `GstVideoFrame`.
    - [x] Remove `queue.write_texture` (the CPU-copy bottleneck).
- [x] **Task 2.3:** Update `crates/clipstrike-render/src/gpu.rs` to create `wgpu::Texture` from the shared GStreamer handle.
    - [x] Use `wgpu::Device::create_texture_from_hal` or FFI to wrap native handles.
    - [x] Implement cross-API synchronization (fences/semaphores) between GStreamer and wgpu.

## Phase 3: Synchronization & UI Wiring
- [x] **Task 3.1:** Implement the `FrameProvider` clock synchronization logic.
- [x] **Task 3.2:** Connect the `mpsc` channel from `clipstrike-media` to `clipstrike-ui`.

## Verification
- Run `cargo check --all-targets`.
- Manually inspect GPU memory usage during playback.

