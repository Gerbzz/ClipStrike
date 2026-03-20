# ClipStrike Media Readback Buffer Reuse

**Goal:** Reduce allocation churn in the remaining CPU readback paths by allowing the GPU readback helpers to fill caller-owned buffers.

**Why this exists:** The media pipeline is still beta-healthy, but the save-frame and thumbnail fallbacks continue to allocate fresh `Vec<u8>` buffers on every invocation. This track keeps the zero-copy / zero-allocation direction moving without pretending the current GStreamer desktop path is fully zero-copy yet.

**Success Criteria:**
- The render readback helper can fill a caller-owned buffer instead of always allocating a fresh `Vec<u8>`.
- The native save-frame path reuses a buffer across calls and still writes the same file output.
- The thumbnail fallback can be refactored onto the same reusable-buffer pattern later without changing behavior.
