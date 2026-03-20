# Specification: Beta Release Core (Track 010)

## Overview
Move ClipStrike from a buildable prototype to a publishable beta by completing the export path, adding beta-critical smoke coverage, and gating incomplete experimental features so they do not block release.

## Goals
- Export a project to a real file from the UI.
- Preserve the last export destination for faster repeat exports.
- Keep beta-critical workflows covered by smoke tests.
- Hide or gate incomplete AI/Vision UI paths that are not required for beta.

## Constraints
- Keep async work on `tokio` channels.
- Avoid global locks.
- Do not add CPU-side pixel processing beyond the existing render copies already required by the current renderer.

## Out of Scope
- UI polish work such as draggable markers and shortcut customization.
- New AI models or higher-level vision features beyond the beta gate.
- Deep zero-copy rewrites that require renderer architecture changes.

## Exit Criteria
- Export can be initiated from the UI and completes with progress and cancel support.
- Save/reopen, playback, and export smoke tests pass.
- Experimental AI/Vision controls are feature-gated or otherwise safe for beta.
