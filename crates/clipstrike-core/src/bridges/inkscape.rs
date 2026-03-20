use std::path::{Path, PathBuf};
use std::process::Command;

pub struct InkscapeBridge {
    executable_path: PathBuf,
}

impl InkscapeBridge {
    pub fn new(executable_path: PathBuf) -> Self {
        Self { executable_path }
    }

    pub fn edit_svg(&self, svg_path: &Path) -> Result<std::process::Child, String> {
        if !self.executable_path.exists() {
            return Err(format!(
                "Inkscape executable not found at {:?}",
                self.executable_path
            ));
        }

        let child = Command::new(&self.executable_path)
            .arg(svg_path)
            .spawn()
            .map_err(|e| format!("Failed to spawn Inkscape: {}", e))?;

        Ok(child)
    }
}
