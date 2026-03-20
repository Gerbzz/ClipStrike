# Tech Stack: ClipStrike

## Language
- **Rust:** Primary language for performance and safety.

## UI Framework
- **eframe / egui:** For a responsive, immediate-mode GUI that works well with GPU-accelerated backends.

## Rendering Engine
- **wgpu:** A cross-platform, safe GPU API based on WebGPU. Used for compositing, effects, and final frame display.

## Media Engine
- **GStreamer:** For robust media decoding, encoding, and processing.
- **playbin3:** Modern GStreamer element for optimized playback pipelines.

## Concurrency & Communication
- **tokio:** For asynchronous tasks and message passing.
- **mpsc channels:** Used for communication between the Video Engine and the UI thread.

## Engineering Mandates
- **Zero Allocations in Render Loop:** Pre-allocate all textures and buffers to prevent frame drops.
- **Zero-Copy Pipeline:** Keep frames in VRAM using `glupload` and `appsink` with raw GPU pointers.
- **No Global Locks:** Ensure thread safety and performance via async communication rather than shared state locks.
