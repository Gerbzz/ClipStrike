use crate::capture_package::{CapturePackage, PackageError};
use crate::project::Project;
use std::io::Write;
use std::path::Path;

/// Create a new editable project from a validated capture package.
/// Existing output files are never overwritten.
pub fn import_package_to_new_project(
    package_dir: &Path,
    output: &Path,
) -> Result<Project, PackageError> {
    let package = CapturePackage::load_and_validate(package_dir)?;
    let first = &package.manifest.clips[0];
    if package.manifest.clips.iter().any(|clip| {
        clip.width != first.width
            || clip.height != first.height
            || u64::from(clip.fps_num) * u64::from(first.fps_den)
                != u64::from(first.fps_num) * u64::from(clip.fps_den)
    }) {
        return Err(PackageError::Invalid(
            "capture clips must share resolution and frame rate".into(),
        ));
    }
    let mut project = Project::new(
        &package.manifest.package_id,
        first.width,
        first.height,
        f64::from(first.fps_num) / f64::from(first.fps_den),
    );
    project.import_capture_package(&package)?;
    project.relink_capture_package(output, &package.manifest.package_id, &package.root)?;
    let bytes = project
        .to_json_bytes()
        .map_err(|error| PackageError::Json(error.to_string()))?;
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| PackageError::Io(error.to_string()))?;
    file.write_all(&bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|error| PackageError::Io(error.to_string()))?;
    file.persist_noclobber(output)
        .map_err(|error| PackageError::Io(format!("{}: {}", output.display(), error.error)))?;
    Ok(project)
}
