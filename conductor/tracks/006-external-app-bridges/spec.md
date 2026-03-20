# Spec: External App Bridges (Track 006)

## Overview
Implement deep integration with professional external tools (Blender and Inkscape) to support 3D animated titles and vector graphics editing, matching OpenShot's signature workflow.

## Requirements
- **Blender Bridge (3D Titles):**
    - Execute `blender.exe` in the background with custom Python scripts.
    - Render 3D text templates to image sequences.
    - Automatic import of rendered sequences into the ClipStrike timeline.
- **Inkscape Bridge (Vector Editing):**
    - "Edit in Inkscape" right-click action for SVG titles.
    - Watch for file modification events from Inkscape.
    - Real-time refresh of the rasterized texture in `wgpu` when the SVG is saved.
- **Path Configuration:** User-friendly UI to set and verify the absolute paths to external executables.

## Success Criteria
- [ ] A user can select a 3D Title template, customize the text, and see it rendered on the timeline via Blender.
- [ ] Right-clicking an SVG title and choosing "Edit in Inkscape" opens the app and reloads the change upon save.
- [ ] Bridges handle missing executables gracefully with clear instructions for the user.
