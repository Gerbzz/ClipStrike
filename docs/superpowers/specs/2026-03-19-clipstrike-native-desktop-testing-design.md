# ClipStrike Native Desktop Testing Harness Design

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reliable native desktop test harness for ClipStrike so egui widget behavior can be tested in-process with `egui_kittest`, then extend coverage with Windows UI Automation smoke tests for the real `clipstrike-app.exe`.

**Architecture:** ClipStrike runs as a native desktop `eframe`/`egui` app with a GStreamer backend. The first pass adds a stable in-process harness around `ClipStrikeApp` so tests can drive buttons, file actions, and timeline state directly. A second pass adds Windows UI Automation for launch-level smoke coverage against the real desktop binary.

**Tech Stack:** Rust, `eframe`, `egui`, `egui_kittest`, Windows UI Automation, existing ClipStrike test hooks, `cargo test`, `cargo check --all-targets`

---

## Context

ClipStrike is not a Tauri app. The native binary is built from `crates/clipstrike-app/src/main.rs` and uses `eframe`/`egui` directly. The current repo already contains test-only hooks in `crates/clipstrike-ui/src/test_hooks.rs` and some in-process UI state capture inside `crates/clipstrike-ui/src/ui.rs`, but the current approach is brittle and still relies on shared global mutex state.

The user-facing failure we are targeting first is the desktop add/import path crash when the `+` button is clicked. The test harness should make that behavior reproducible in Rust tests before we touch more UI logic.

## Design Principles

- Test the native desktop app first.
- Keep the first harness small and focused on widget/state behavior.
- Make selectors and test hooks stable enough to survive UI refactors.
- Avoid adding more global mutex-based test plumbing.
- Use TDD for every new behavior: write the failing test first, verify it fails, then implement the minimum fix.
- Keep OS-level smoke tests separate from in-process egui tests.

## Option Review

### Option 1: `egui_kittest` only

This gives fast, deterministic tests inside the app process. It is the best first step for button behavior, asset insertion, and editor state transitions. It does not prove the native app launches and stays healthy under the real desktop environment.

### Option 2: Windows UI Automation only

This proves the real window and input path, but it is slower and more brittle than an in-process egui harness. It also makes debugging harder because failures are split between the app and the OS automation layer.

### Option 3: Layered approach

Use `egui_kittest` first, then add Windows UI Automation smoke tests once the widget/state harness is stable. This gives the fastest feedback loop for logic regressions and a second layer for real desktop launch coverage.

**Recommendation:** Option 3. It fits ClipStrike's stack and keeps the test pyramid sane.

## Architecture

### In-process harness

Add a focused test harness around `ClipStrikeApp` that can:

- construct the app in a test mode
- drive egui widgets by label or stable test identifier
- capture state snapshots for assertions
- trigger the existing add/import/export code paths without a full GUI session

This harness should live alongside the UI crate so it can directly observe the editor state and widget tree. It should be minimal, explicit, and easy to reuse for future regression tests.

### Native smoke tests

Add Windows-specific smoke tests that:

- launch `clipstrike-app.exe`
- wait for the main window
- confirm the process stays alive
- exercise the top-level desktop actions that are hard to validate in-process

These tests are not the first deliverable. They come after the egui harness is stable and the current crash path is covered.

## File Boundaries

- Modify: `crates/clipstrike-ui/src/ui.rs`
- Modify: `crates/clipstrike-ui/src/test_hooks.rs`
- Add: `crates/clipstrike-ui/tests/egui_harness.rs`
- Add: `crates/clipstrike-ui/tests/native_smoke.rs`
- Possibly modify: `crates/clipstrike-app/src/lib.rs` if a test entrypoint needs to be exposed more cleanly

## Phase 1: `egui_kittest` Harness

### Task 1: Stabilize test entrypoints

Make sure the UI crate exposes a test-friendly constructor or harness path without dragging in desktop-only code. Keep the constructor small and deterministic.

### Task 2: Replace brittle shared-state hooks

The current `test_hooks.rs` uses global mutexes. Replace that with a test-safe path that is owned by the app instance or routed through explicit test commands. If the existing hook surface must remain temporarily, isolate it behind the smallest possible boundary and plan a follow-up removal.

### Task 3: Add the first regression test

Write an `egui_kittest` test that reproduces the add/import behavior and asserts:

- the click is handled
- the app does not crash
- the selected media is inserted into the expected state
- the editor state snapshot changes in the expected way

The initial test should be narrowly scoped to the visible crash path, not a broad UI audit.

### Task 4: Make the minimal fix

Implement only enough code to make the new test pass. Do not expand the feature surface while fixing the regression.

### Task 5: Verify

Run:

```bash
cargo test -p clipstrike-ui
cargo check --all-targets
```

Expected:
- the new regression test passes
- no new warnings or failures are introduced by the harness changes

## Phase 2: Windows UI Automation Smoke Tests

### Task 6: Launch verification

Create a Windows-only smoke test that launches `clipstrike-app.exe` with the documented GStreamer environment and verifies the main window appears.

### Task 7: Core interaction smoke test

Add one UIA test that clicks the top-level import/add action and checks that the native app stays alive and emits the expected state transition.

### Task 8: Verify the real desktop path

Run the desktop smoke tests against the native binary and record failures separately from in-process egui tests.

## Testing Strategy

- Write one failing test per behavior.
- Use real app code when possible, not mocks.
- Keep tests narrow: one click, one state change, one assertion set.
- Prefer deterministic assertions against app state snapshots over pixel checks.
- Use OS-level automation only where in-process tests cannot reach the behavior.

## Error Handling

- If a test reveals a crash, capture the smallest repro test first.
- If the same failure repeats, stop and fix the harness boundary instead of layering more test code.
- If a Windows UI Automation test fails because of environment setup, record the exact launcher/env requirement in `docs/LOCAL_SETUP.md`.

## Success Criteria

- `egui_kittest` can drive at least one real ClipStrike widget flow in-process.
- The current add/import crash is covered by an automated regression test.
- The desktop app still passes `cargo check --all-targets`.
- A native UIA smoke test can launch `clipstrike-app.exe` and confirm the main window is alive.
- The desktop path remains the source of truth for backend behavior.
