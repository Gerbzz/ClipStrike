# Product: ClipStrike

## Vision
ClipStrike is a high-performance, professional-grade video editor designed as a 1:1 OpenShot clone, rewritten from the ground up in Rust. It aims to provide the same user-friendly experience as OpenShot but with significantly higher performance, stability, and modern hardware utilization.

## Core Goals
- **1:1 Feature Parity with OpenShot:** Every feature available in OpenShot 3.x should be available in ClipStrike.
- **Extreme Performance:** Leverage Rust's safety and performance, along with modern GPU-accelerated rendering.
- **Hardware Acceleration:** Full utilization of NVDEC, D3D11, and other hardware decoders via GStreamer.
- **Cross-Platform:** Target Windows, Linux, and macOS (with initial focus on Windows/native).

## High-Level Features
- Multi-track timeline.
- Real-time video preview.
- Support for most video, audio, and image formats.
- Powerful keyframe-based animations.
- Video effects and transitions.
- SVG-based titles.
- Fast and reliable export engine.
