use clipstrike_core::project::Project;
use clipstrike_ui::ui::{EditorHostResources, EmbeddedEditorWorkspace};
use std::path::Path;

#[test]
fn host_can_compile_against_the_public_after_startup_editor_contract() {
    let _: for<'a> fn(&eframe::CreationContext<'a>) -> Result<EditorHostResources, String> =
        EditorHostResources::capture;
    let _: fn(&EditorHostResources, &Path) -> Result<EmbeddedEditorWorkspace, String> =
        EditorHostResources::open_project;
    let _: fn(&EditorHostResources) -> EditorHostResources = Clone::clone;
    let _: fn(&mut EmbeddedEditorWorkspace, &egui::Context) = EmbeddedEditorWorkspace::show;
    let _: fn(&EmbeddedEditorWorkspace) -> Option<&Path> = EmbeddedEditorWorkspace::project_path;
    let _: fn(&EmbeddedEditorWorkspace) -> Project = EmbeddedEditorWorkspace::project_snapshot;
    let _: fn(&EmbeddedEditorWorkspace) -> bool = EmbeddedEditorWorkspace::has_unsaved_changes;
    let _: fn(&EmbeddedEditorWorkspace) -> bool = EmbeddedEditorWorkspace::is_exporting;
    let _: fn(&mut EmbeddedEditorWorkspace) -> bool = EmbeddedEditorWorkspace::take_return_request;
}
