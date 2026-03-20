# Spec: Professional FX & LUTs (Track 004)

## Overview
Expand the `wgpu` compute shader FX pipeline to include professional-grade color grading tools and support for standard LUT files, matching OpenShot's advanced filters.

## Requirements
- **Advanced Grading Shaders:** 
    - **Gamma:** WGSL power-function based correction.
    - **Hue:** HSV-based color rotation.
    - **Greyscale:** Accurate luma-based desaturation.
- **Chroma Key:** High-quality green/blue screen removal with threshold and fuzziness controls.
- **LUT Support:** 
    - Import and parse `.cube` files (1D and 3D LUTs).
    - Map LUT data to a 3D texture in `wgpu` for high-speed color mapping.
- **Effects Browser:** A dedicated panel to browse, search, and categorize available filters.
- **Drag-and-Drop FX:** Visual interaction to apply effects directly to clips on the timeline.

## Success Criteria
- [ ] Users can remove a green screen from a video using the Chroma Key filter.
- [ ] Cinematic color grading can be applied via a `.cube` LUT file.
- [ ] Effects parameters are fully keyframeable and animate smoothly.
- [ ] All effects run on the GPU with zero CPU involvement per frame.
