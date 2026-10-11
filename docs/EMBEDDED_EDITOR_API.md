# Embedding ClipStrike in a Rust host

`clipstrike_ui::ui::EmbeddedEditorWorkspace` draws the same editor as the standalone ClipStrike application in a host eframe window. The host retains ownership of the window, capture flow, and navigation. ClipStrike owns editing, save/reopen, preview, and export while its workspace is shown. DemoStrike is an intended host; its integration has not been verified.

## Host setup

- Use eframe 0.30 with `eframe::Renderer::Wgpu`. `EmbeddedEditorWorkspace::open` returns an error if WGPU is unavailable. A Glow host must change its renderer before opening the editor.
- Depend on `clipstrike-ui` with `gstreamer-backend` for real native playback and export. Install the official GStreamer MSVC runtime and development files, and make `ffmpeg` and `ffprobe` available for native export verification.
- Pass the saved `.cstrike` path produced from a DemoStrike capture package. Keep that project's referenced media available. Opening reports invalid projects as an error; missing capture sources are shown in the editor as offline warnings.
- Draw the workspace for the whole frame. Do not draw another root egui panel around it. Keep calling `show` while `is_exporting()` is true so export capture and finalization advance.

The package handoff is `clipstrike_core::capture_import::import_package_to_new_project(&package_dir, &new_project_path) -> Result<Project, PackageError>`. It validates the package and writes a new `.cstrike` without overwriting an existing file. DemoStrike already depends on `clipstrike-core`. Add `clipstrike-ui = { path = "../clipstrike/crates/clipstrike-ui", optional = true, features = ["gstreamer-backend"] }` to its manifest and include `"dep:clipstrike-ui"` in DemoStrike's `native-ui` feature. This path example assumes sibling local checkouts; a host should pin a compatible ClipStrike revision in its own build configuration. Both projects currently use eframe/egui 0.30, and ClipStrike's root workspace pins wgpu 23.0. The host's `NativeOptions` must select `Renderer::Wgpu`.

```rust
use clipstrike_ui::ui::{EditorHostResources, EmbeddedEditorWorkspace};

// In the host's eframe creation callback:
let editor_resources = EditorHostResources::capture(cc)?;

// Later, after a capture package has been imported and saved:
let mut editor = editor_resources.open_project(&saved_project_path)?;

// In the host's eframe::App::update, while the editor view is active:
editor.show(ctx);
if editor.take_return_request() {
    if editor.is_exporting() {
        // Keep showing the editor until export completes or the user cancels it.
    } else if editor.has_unsaved_changes() {
        // Show the host's save/discard/cancel decision before dropping editor.
    } else {
        // Return to the capture workspace.
    }
}
```

The example is the handoff pattern, not a complete host application. `project_path()` exposes the current save target and `project_snapshot()` provides a copy of the live edit if the host needs project metadata. File > Return to Capture raises a one-shot request instead of closing the host window. Standalone File > Quit still closes its own window. Embedded edits do not persist ClipStrike's standalone recent-project, theme, or fullscreen configuration.

`EditorHostResources` owns clones of the egui context and WGPU render state; it does not borrow `CreationContext`. The host can retain it across frames and open another saved project after returning to capture. `EmbeddedEditorWorkspace::open(cc, path)` remains a startup convenience. Dropping a workspace cancels preparation/finalization workers, stops an active encoder, removes its staging file, and unregisters its 12 native textures. Unpublished stages produced later by a canceled worker are owned by a cleanup guard and cannot be published after the workspace is dropped. The host should still offer a save/discard decision for unsaved edits before dropping.

The editor's Export Project menu drives its native render pipeline. For a noninteractive saved-project export, `clipstrike_ui::ui::ClipStrikeApp::export_saved_project_offscreen(&project_path, &output_path, clipstrike_media::export::ExportSettings)` is available on native `gstreamer-backend` builds and returns an error instead of reporting success for a missing backend. Add `clipstrike-media` as a host dependency only if calling that offscreen hook. The export toolchain needs the media backend and local FFmpeg/FFprobe.

## Verified and outstanding

The workspace constructor and frame delegation have focused egui tests. The return request test confirms that the embedded action issues no viewport Close command. A real WGPU test opens a project three times after startup, checks all 12 textures are registered then freed on every close, and confirms the host's light theme and font size survive construction and editor frames. Active encoder and finalization close tests cover staging cleanup. The native GStreamer workspace suite and a saved-project offscreen export passed; the output was independently probed and fully decoded. This does not prove that DemoStrike has connected the API, that pointer interaction works in its native window, or that speakers played audio. Supported Computer Use currently reports no native app surfaces (`apps: []`), so visual UI acceptance remains open.
