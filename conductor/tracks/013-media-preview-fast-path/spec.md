# Spec: Media Preview Fast Path

ClipStrike should recognize when a decoded frame is external-backed and the clip has no active visual effects, then present the raw frame texture directly instead of forcing the FX offscreen pass. This keeps the zero-copy metadata meaningful in the preview loop while preserving the existing FX pipeline for clips that actually need it.

Success criteria:
- A unit test proves the bypass decision is true for an external-backed frame with neutral clip properties.
- A unit test proves the bypass decision is false when any clip effect is active.
- The preview path still falls back to the current FX pipeline for all non-external or effectful frames.
- `cargo check --all-targets` passes after the change.
