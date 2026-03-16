use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_RECENT_PROJECTS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThemePreference {
    Dark,
    Light,
}

impl Default for ThemePreference {
    fn default() -> Self {
        Self::Dark
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProjectProfile {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub framerate: f64,
}

impl Default for ProjectProfile {
    fn default() -> Self {
        Self {
            name: "HD 1080p 30 fps".to_string(),
            width: 1920,
            height: 1080,
            framerate: 30.0,
        }
    }
}

impl ProjectProfile {
    pub fn presets() -> Vec<Self> {
        vec![
            Self {
                name: "Web 720p 30 fps".to_string(),
                width: 1280,
                height: 720,
                framerate: 30.0,
            },
            Self {
                name: "HD 1080p 30 fps".to_string(),
                width: 1920,
                height: 1080,
                framerate: 30.0,
            },
            Self {
                name: "HD 1080p 60 fps".to_string(),
                width: 1920,
                height: 1080,
                framerate: 60.0,
            },
            Self {
                name: "4K UHD 30 fps".to_string(),
                width: 3840,
                height: 2160,
                framerate: 30.0,
            },
            Self {
                name: "4K UHD 60 fps".to_string(),
                width: 3840,
                height: 2160,
                framerate: 60.0,
            },
            Self {
                name: "Square 1080 30 fps".to_string(),
                width: 1080,
                height: 1080,
                framerate: 30.0,
            },
        ]
    }

    pub fn aspect_ratio_label(&self) -> &'static str {
        match (self.width, self.height) {
            (w, h) if w * 9 == h * 16 => "16:9",
            (w, h) if w == h => "1:1",
            (w, h) if w * 3 == h * 4 => "4:3",
            _ => "Custom",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub recent_projects: Vec<PathBuf>,
    pub default_project_profile: ProjectProfile,
    pub advanced_view: bool,
    pub fullscreen: bool,
    pub autosave_enabled: bool,
    pub autosave_interval_minutes: u32,
    pub theme: ThemePreference,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            recent_projects: Vec::new(),
            default_project_profile: ProjectProfile::default(),
            advanced_view: true,
            fullscreen: false,
            autosave_enabled: false,
            autosave_interval_minutes: 5,
            theme: ThemePreference::Dark,
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            PathBuf::from(appdata).join("ClipStrike")
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(".clipstrike")
        }
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load_or_default() -> Self {
        Self::load_from_path(Self::config_path()).unwrap_or_default()
    }

    pub fn load_from_path<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to_path(Self::config_path())
    }

    pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn remember_recent_project<P: Into<PathBuf>>(&mut self, path: P) {
        let path = path.into();
        self.recent_projects.retain(|existing| existing != &path);
        self.recent_projects.insert(0, path);
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn remember_recent_projects_dedupes_and_limits() {
        let mut config = AppConfig::default();
        for i in 0..12 {
            config.remember_recent_project(PathBuf::from(format!("project-{i}.cstrike")));
        }
        config.remember_recent_project(PathBuf::from("project-5.cstrike"));

        assert_eq!(config.recent_projects.len(), 10);
        assert_eq!(
            config.recent_projects[0],
            PathBuf::from("project-5.cstrike")
        );
        assert_eq!(
            config
                .recent_projects
                .iter()
                .filter(|path| **path == PathBuf::from("project-5.cstrike"))
                .count(),
            1
        );
    }

    #[test]
    fn save_and_load_round_trip() {
        let mut config = AppConfig::default();
        config.remember_recent_project(PathBuf::from("alpha.cstrike"));
        config.remember_recent_project(PathBuf::from("beta.cstrike"));
        config.advanced_view = false;
        config.fullscreen = true;
        config.autosave_enabled = true;
        config.autosave_interval_minutes = 3;
        config.theme = ThemePreference::Light;
        config.default_project_profile = ProjectProfile {
            name: "4K UHD 60 fps".to_string(),
            width: 3840,
            height: 2160,
            framerate: 60.0,
        };

        let path = temp_path("clipstrike-app-config-test.json");
        config.save_to_path(&path).expect("failed to save config");
        let loaded = AppConfig::load_from_path(&path).expect("failed to load config");

        assert_eq!(loaded, config);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn profile_presets_include_standard_formats() {
        let presets = ProjectProfile::presets();
        assert!(
            presets
                .iter()
                .any(|preset| preset.width == 1920 && preset.height == 1080)
        );
        assert!(
            presets
                .iter()
                .any(|preset| preset.width == 3840 && preset.height == 2160)
        );
        assert!(
            presets
                .iter()
                .any(|preset| preset.aspect_ratio_label() == "1:1")
        );
    }
}
