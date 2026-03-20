# ClipStrike Native Desktop Testing Harness Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reliable native desktop test harness for ClipStrike so egui widget behavior can be tested in-process with `egui_kittest`, then extend coverage with Windows UI Automation smoke tests for the real `clipstrike-app.exe`.

**Architecture:** Keep the implementation focused on the native desktop app. The first implementation pass adds a test-friendly path around `ClipStrikeApp` and the UI command handling so we can reproduce the add/import crash in-process. The second pass adds OS-level smoke tests against the real desktop executable to prove the launch path and top-level interactions.

**Tech Stack:** Rust, `eframe`, `egui`, `egui_kittest`, Windows UI Automation, existing ClipStrike test hooks, `cargo test`, `cargo check --all-targets`

---

## Chunk 1: In-Process egui Harness

### Task 1: Add the test harness dependency and entrypoint

**Files:**
- Modify: `crates/clipstrike-ui/Cargo.toml`
- Modify: `crates/clipstrike-ui/src/ui.rs`
- Possibly modify: `crates/clipstrike-app/src/lib.rs`

- [x] **Step 1: Write the failing test**

Add a minimal `egui_kittest` regression test that drives the native UI and asserts the add/import action is handled without crashing. The test should target the current `+` add-media path and fail before the fix exists.

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p clipstrike-ui add_import_button_does_not_crash -- --nocapture`
Expected: FAIL because the test harness or the stable widget entrypoint does not exist yet.

- [x] **Step 3: Add the minimal test entrypoint**

Expose a small test-friendly constructor or harness path for `ClipStrikeApp` that can be created in tests. If a new helper module is needed, keep it small and local to the UI crate.

- [x] **Step 4: Run the test to verify it passes**

Run: `cargo test -p clipstrike-ui add_import_button_does_not_crash -- --nocapture`
Expected: PASS.

- [x] **Step 5: Verify build health**

Run: `cargo check --all-targets`
Expected: PASS with no new build errors.

### Task 2: Replace the brittle test hook state

**Files:**
- Modify: `crates/clipstrike-ui/src/test_hooks.rs`
- Modify: `crates/clipstrike-ui/src/ui.rs`
- Add: `crates/clipstrike-ui/tests/egui_harness.rs`

- [x] **Step 1: Write the failing test**

Add a second regression test that asserts the state snapshot is updated through the new harness path instead of the current global mutex-based test hooks.

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p clipstrike-ui harness_state_snapshot_updates_without_global_mutex -- --nocapture`
Expected: FAIL because the snapshot path is not yet harness-owned.

- [x] **Step 3: Move the state capture behind the harness**

Replace the shared `Mutex<Vec<String>>` / `Mutex<String>` hook pattern with the smallest possible app-owned or test-owned command/state sink. Keep the temporary compatibility surface minimal if older tests still depend on it.

- [x] **Step 4: Run the test to verify it passes**

Run: `cargo test -p clipstrike-ui harness_state_snapshot_updates_without_global_mutex -- --nocapture`
Expected: PASS.

- [x] **Step 5: Re-run the suite**

Run: `cargo test -p clipstrike-ui`
Expected: PASS.

### Task 3: Add the add/import crash regression test

**Files:**
- Add: `crates/clipstrike-ui/tests/egui_harness.rs`
- Modify: `crates/clipstrike-ui/src/ui.rs`

- [x] **Step 1: Write the failing test**

Write the real regression test for the current crash path: clicking the add/import button in the desktop UI should choose an existing track, insert the item, and not panic.

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p clipstrike-ui add_media_uses_existing_track_and_does_not_crash -- --nocapture`
Expected: FAIL because the button path still needs harness coverage or a safe selection path.

- [x] **Step 3: Make the minimal fix**

Adjust the add/import path so it is safe under the harness and uses stable state assumptions. Do not widen the behavior surface beyond what the test needs.

- [x] **Step 4: Run the test to verify it passes**

Run: `cargo test -p clipstrike-ui add_media_uses_existing_track_and_does_not_crash -- --nocapture`
Expected: PASS.

- [x] **Step 5: Verify build health**

Run: `cargo check --all-targets`
Expected: PASS.

## Chunk 2: Desktop Smoke Tests

### Task 4: Add Windows launch verification

**Files:**
- Add: `crates/clipstrike-ui/tests/native_smoke.rs`
- Possibly modify: `run_clipstrike_gstreamer.cmd`
- Possibly modify: `docs/LOCAL_SETUP.md`

- [x] **Step 1: Write the failing test**

Add a Windows-only smoke test that launches `clipstrike-app.exe` with the documented GStreamer environment and verifies the process/window appears.

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p clipstrike-ui windows_launch_smoke -- --nocapture`
Expected: FAIL or be ignored on non-Windows, and fail on Windows until the harness exists.

- [x] **Step 3: Implement the minimal smoke harness**

Use Windows UI Automation or the narrowest viable desktop driver to launch the native executable and confirm it stays alive.

- [x] **Step 4: Run the test to verify it passes**

Run: `cargo test -p clipstrike-ui windows_launch_smoke -- --nocapture`
Expected: PASS on Windows.

- [x] **Step 5: Verify build health**

Run: `cargo check --all-targets`
Expected: PASS.

### Task 5: Add a real desktop interaction smoke test

**Files:**
- Add: `crates/clipstrike-ui/tests/native_smoke.rs`
- Modify: `crates/clipstrike-ui/src/ui.rs`

- [x] **Step 1: Write the failing test**

Add one UIA smoke test that clicks the top-level import/add control and confirms the app emits the expected state transition.

- [x] **Step 2: Run the test to verify it fails**

Run: `cargo test -p clipstrike-ui windows_import_smoke -- --nocapture`
Expected: FAIL before the interaction is wired.

- [x] **Step 3: Make the minimal UI or test-hook change**

Add the smallest stable identifier or command path needed for the automation to find the control without using pixels.

- [x] **Step 4: Run the test to verify it passes**

Run: `cargo test -p clipstrike-ui windows_import_smoke -- --nocapture`
Expected: PASS on Windows.

- [x] **Step 5: Verify build health**

Run: `cargo check --all-targets`
Expected: PASS.

## Verification Gate

Before marking this plan complete:

- `cargo test -p clipstrike-ui` passes
- `cargo check --all-targets` passes
- The native add/import crash is covered by a regression test
- The desktop launch smoke test exists and runs on Windows
- The desktop path remains the source of truth for backend behavior

## Notes for Implementers

- Do not add new global mutexes for test state.
- Keep the harness focused on the first visible crash path.
- Prefer explicit test helpers over broad UI refactors.
- If a step reveals a new crash, add the failing test first and fix only that failure.
