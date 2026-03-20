use std::path::{Path, PathBuf};
use std::process::Command;

pub struct BlenderBridge {
    executable_path: PathBuf,
}

impl BlenderBridge {
    pub fn new(executable_path: PathBuf) -> Self {
        Self { executable_path }
    }

    pub fn render_title(
        &self,
        template_path: &Path,
        script_path: &Path,
        output_dir: &Path,
        text: &str,
    ) -> Result<std::process::Child, String> {
        if !self.executable_path.exists() {
            return Err(format!(
                "Blender executable not found at {:?}",
                self.executable_path
            ));
        }

        // command: blender -b template.blend -P script.py -- --output-dir /path/to/out --text "hello"
        let child = Command::new(&self.executable_path)
            .arg("-b")
            .arg(template_path)
            .arg("-P")
            .arg(script_path)
            .arg("--")
            .arg("--output-dir")
            .arg(output_dir)
            .arg("--text")
            .arg(text)
            .spawn()
            .map_err(|e| format!("Failed to spawn Blender: {}", e))?;

        Ok(child)
    }
}
