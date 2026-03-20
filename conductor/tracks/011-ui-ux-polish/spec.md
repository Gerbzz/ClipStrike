# UI/UX Polish Track

**Goal:** Add crash recovery checkpoints for unsaved work so ClipStrike can preserve the user's session across unexpected exits, then use the same recovery path to support later UI/UX polish work.

**Why this exists:** The beta release core is complete, but the app still lacks a session checkpoint/recovery path. Autosave settings already exist in app config, so the remaining gap is wiring recovery around the current save/load model without introducing new global locks or CPU-side media work.

**Success Criteria:**
- Recovery checkpoint paths are deterministic and testable.
- The native desktop app can write a recovery checkpoint when autosave is enabled.
- The app can detect and surface recovery state on startup without breaking normal open/save behavior.
- Native desktop smoke coverage still passes after each recovery-related change.

**Constraints:**
- Keep async work on `tokio` channels.
- Avoid global locks.
- Do not add CPU-side pixel processing.
- Keep the recovery path orthogonal to the native desktop state flow.
