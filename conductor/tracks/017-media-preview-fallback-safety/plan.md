# Media Preview Fallback Safety Plan

**Objective:** Keep preview correct when GStreamer produces GL-backed samples by preserving the existing CPU upload fallback while still retaining external texture metadata for future import work.

## Step 1
- [x] Add a helper that can construct a frame upload request from a buffer plus optional external texture metadata.
- [x] Add a focused unit test that proves metadata and pixel payload coexist for readable samples.

## Step 2
- [x] Use the helper in the native GStreamer engine.
- [x] Keep the CPU fallback branch intact so preview and smoke tests continue to pass.

## Step 3
- [x] Run the focused GStreamer backend test.
- [x] Re-run `cargo check --all-targets`.
- [x] Update conductor status and implementation state notes.
