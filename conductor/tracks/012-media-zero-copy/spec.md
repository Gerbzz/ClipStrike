# Media Zero-Copy Track

**Goal:** Finish the remaining media-path work by turning GStreamer GL-backed samples into explicit texture metadata and preparing the render path for eventual zero-copy interop.

**Why this exists:** Track 011 closed the media hardening and readback cleanup work, but the workspace still has an open engineering-rule gap: the GStreamer backend can recognize GL-backed samples, yet the render path still needs a real import hook before the media pipeline is truly zero-copy.

**Success Criteria:**
- GL-backed samples carry `external_texture_id` metadata without forcing a CPU map.
- The preview path has a clear, testable boundary for external texture import.
- The native desktop GStreamer app keeps launching and passing smoke tests after each change.
