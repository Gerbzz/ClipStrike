# ClipStrike Media Thumbnail Buffer Reuse

**Goal:** Reduce allocation churn in the thumbnail fallback path by allowing the GStreamer readback helper to fill a caller-owned buffer.

**Why this exists:** The thumbnail pipeline still allocates a fresh `Vec<u8>` for every thumbnail. That is acceptable as a fallback, but it is still part of the zero-copy / zero-allocation cleanup that remains open after the render/save-frame buffer reuse work.

**Success Criteria:**
- The thumbnail readback helper can fill a caller-owned buffer instead of always allocating a fresh `Vec<u8>`.
- The owned thumbnail API remains available for existing call sites.
- The helper is verified with focused unit tests and the backend env.
