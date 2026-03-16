# Multi-Agent Prompts for ClipStrike

To get the most out of multiple AI agents working simultaneously, you must ensure they are working on **separate files** so they do not overwrite each other's code.

Here are 4 prompts you can copy and paste into 4 separate agent windows right now.

---

### Agent 1: The UI & Window Engine (Phase 1)
**File focus:** `src/main.rs` and `src/ui.rs`
**Prompt to Copy:**
> Please start working on Phase 1 for our ClipStrike Rust project. We've initialized the cargo project. I need you to set up `eframe` with a `wgpu` backend as the primary windowing engine. Set up a basic `egui` OpenShot-style 1:1 UI layout. Do not worry about video decoding right now, just get the UI and hardware-accelerated window context working. Please keep all your changes contained within `src/main.rs` and a new `src/ui.rs` file so you don't conflict with other agents.

---

### Agent 2: Timeline Engine (Phase 3)
**File focus:** `src/timeline.rs` 
**Prompt to Copy:**
> Please start working on the core logic for Phase 3 (Timeline Data Structures) for our ClipStrike Rust project. Create a new independent file called `src/timeline.rs`. Implement an Interval Tree data structure for querying what video clips exist at timestamp X (O(log N) queries). Also, implement a basic "Snap Engine" using a spatial hash for calculating magnetic attraction between clip edges. Do not touch `src/main.rs` or any UI code; just build these backend data structures and write basic unit tests for them.

---

### Agent 3: Project State & Async Messaging (Phase 5)
**File focus:** `src/project.rs`
**Prompt to Copy:**
> Please start working on parts of Phase 5 for our ClipStrike Rust project. Create a new independent file called `src/project.rs`. Implement `serde`-based JSON serialization to save and load `.cstrike` project files. Also, design the Tokio message passing channels (`tokio::sync::mpsc`) that will be used to send commands from the UI to the background Engine. Do not touch `src/main.rs` or any UI code; just build the project state structs and write unit tests.

---

### Agent 4: GStreamer Metadata Probe (Phase 2)
**File focus:** `src/probe.rs`
**Prompt to Copy:**
> Please start working on Task 2.4 from Phase 2 for our ClipStrike Rust project. Create a new independent file called `src/probe.rs`. Build a background "Metadata Probe" using `gst-discoverer` (via the `gstreamer` Rust bindings). It needs to extract video clip info (FPS, resolution, duration) efficiently. Write it asynchronously so it doesn't block. Do not touch `src/main.rs` or any UI code; just build the probe logic.
