use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

pub const CAPTURE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureManifest {
    pub schema_version: u32,
    pub package_id: String,
    #[serde(default)]
    pub media_validation: MediaValidation,
    pub clips: Vec<CaptureClip>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MediaValidation {
    #[default]
    Structural,
    StreamProbed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaptureClip {
    pub clip_id: String,
    pub relative_path: String,
    pub sha256: String,
    pub demo_sha256: String,
    #[serde(default)]
    pub demo_path_hint: Option<String>,
    pub event_id: String,
    pub player_steamid64: String,
    pub map: String,
    pub round: u32,
    pub utility_type: String,
    pub start_tick: i64,
    pub end_tick: i64,
    pub camera_role: String,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub audio: AudioEvidence,
    pub qc_status: QcStatus,
    pub group_order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioEvidence {
    pub source: AudioSource,
    pub channels: u16,
    pub sample_rate: u32,
    pub verified: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioSource {
    HlaeWav,
    ScreenCapture,
    GeneratedSilence,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QcStatus {
    Accepted,
    Rejected,
}

#[derive(Debug)]
pub enum PackageError {
    Io(String),
    Json(String),
    Invalid(String),
    HashMismatch(String),
    UnsupportedVersion(u32),
    Cancelled(String),
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message)
            | Self::Json(message)
            | Self::Invalid(message)
            | Self::HashMismatch(message)
            | Self::Cancelled(message) => formatter.write_str(message),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported capture package version {version}")
            }
        }
    }
}

impl std::error::Error for PackageError {}

#[derive(Debug, Clone)]
pub struct CapturePackage {
    pub root: PathBuf,
    pub manifest: CaptureManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapturedAsset {
    pub asset_id: usize,
    pub package_id: String,
    #[serde(default)]
    pub package_path: Option<String>,
    #[serde(default)]
    pub media_validation: MediaValidation,
    pub clip: CaptureClip,
}

impl CapturePackage {
    pub fn load_and_validate(root: &Path) -> Result<Self, PackageError> {
        Self::load_and_validate_with_cancel(root, || false)
    }

    pub fn load_and_validate_with_cancel(
        root: &Path,
        should_cancel: impl Fn() -> bool,
    ) -> Result<Self, PackageError> {
        let package = Self::load_manifest_structure(root)?;
        let mut resolved_media_paths = HashSet::new();
        for clip in &package.manifest.clips {
            let media_path = package.validate_media_clip_with_cancel(clip, &should_cancel)?;
            if !resolved_media_paths.insert(media_path) {
                return Err(PackageError::Invalid(format!(
                    "capture clips must reference separate files: {}",
                    clip.relative_path
                )));
            }
        }
        Ok(package)
    }

    /// Validate the manifest independently of media availability. Relinking uses
    /// this once, then checks each stored asset so one bad take stays isolated.
    pub fn load_manifest_structure(root: &Path) -> Result<Self, PackageError> {
        let root = root
            .canonicalize()
            .map_err(|error| PackageError::Io(error.to_string()))?;
        let bytes = std::fs::read(root.join("manifest.json"))
            .map_err(|error| PackageError::Io(error.to_string()))?;
        let manifest: CaptureManifest = serde_json::from_slice(&bytes)
            .map_err(|error| PackageError::Json(error.to_string()))?;
        if manifest.schema_version != CAPTURE_SCHEMA_VERSION {
            return Err(PackageError::UnsupportedVersion(manifest.schema_version));
        }
        if manifest.package_id.trim().is_empty() || manifest.clips.is_empty() {
            return Err(PackageError::Invalid(
                "package ID and clips are required".into(),
            ));
        }

        let mut clip_ids = HashSet::new();
        let mut media_paths = HashSet::new();
        let mut existing_resolved_paths = HashSet::new();
        let mut group_orders = HashSet::new();
        for clip in &manifest.clips {
            if !clip_ids.insert(clip.clip_id.clone())
                || !media_paths.insert(clip.relative_path.to_lowercase())
                || !group_orders.insert(clip.group_order)
            {
                return Err(PackageError::Invalid(
                    "clip IDs, paths and group orders must be unique".into(),
                ));
            }
            validate_clip_fields(clip)?;
            let relative = Path::new(&clip.relative_path);
            if relative.as_os_str().is_empty()
                || !relative
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
            {
                return Err(PackageError::Invalid(format!(
                    "unsafe clip path: {}",
                    clip.relative_path
                )));
            }
            // A missing take is handled per asset, but present symlink aliases
            // still invalidate the package's independent-file guarantee.
            if let Ok(resolved) = root.join(relative).canonicalize() {
                if !resolved.starts_with(&root) || !existing_resolved_paths.insert(resolved) {
                    return Err(PackageError::Invalid(format!(
                        "capture clip path escapes or aliases another file: {}", clip.relative_path
                    )));
                }
            }
        }
        Ok(Self { root, manifest })
    }

    pub fn validate_media_clip_with_cancel(
        &self,
        clip: &CaptureClip,
        should_cancel: impl Fn() -> bool,
    ) -> Result<PathBuf, PackageError> {
        if should_cancel() {
            return Err(PackageError::Cancelled("capture package validation cancelled".into()));
        }
        let media_path = self.root.join(&clip.relative_path).canonicalize()
            .map_err(|error| PackageError::Io(format!("{}: {error}", clip.relative_path)))?;
        if !media_path.starts_with(&self.root) || !media_path.is_file() {
            return Err(PackageError::Invalid(format!(
                "clip escapes package or is not a file: {}", clip.relative_path
            )));
        }
        let actual_hash = sha256_file_with_cancel(&media_path, should_cancel)?;
        if !actual_hash.eq_ignore_ascii_case(&clip.sha256) {
            return Err(PackageError::HashMismatch(format!("media hash mismatch: {}", clip.clip_id)));
        }
        Ok(media_path)
    }

    pub fn media_path(&self, clip: &CaptureClip) -> PathBuf {
        self.root.join(&clip.relative_path)
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn validate_clip_fields(clip: &CaptureClip) -> Result<(), PackageError> {
    if clip.clip_id.trim().is_empty()
        || clip.event_id.trim().is_empty()
        || clip.player_steamid64.trim().is_empty()
        || clip.map.trim().is_empty()
        || clip.utility_type.trim().is_empty()
        || clip.camera_role.trim().is_empty()
        || !valid_sha256(&clip.sha256)
        || !valid_sha256(&clip.demo_sha256)
        || clip.end_tick <= clip.start_tick
        || clip.duration_ms == 0
        || clip.width == 0
        || clip.height == 0
        || clip.fps_num == 0
        || clip.fps_den == 0
    {
        return Err(PackageError::Invalid(format!(
            "invalid clip metadata: {}",
            clip.clip_id
        )));
    }
    if clip.qc_status != QcStatus::Accepted
        || clip.audio.source != AudioSource::HlaeWav
        || !clip.audio.verified
        || clip.audio.channels == 0
        || clip.audio.sample_rate == 0
    {
        return Err(PackageError::Invalid(format!(
            "clip lacks accepted real audio evidence: {}",
            clip.clip_id
        )));
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String, PackageError> {
    sha256_file_with_cancel(path, || false)
}

pub fn sha256_file_with_cancel(
    path: &Path,
    should_cancel: impl Fn() -> bool,
) -> Result<String, PackageError> {
    let mut file = File::open(path).map_err(|error| PackageError::Io(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if should_cancel() {
            return Err(PackageError::Cancelled(format!("hashing cancelled: {}", path.display())));
        }
        let count = file
            .read(&mut buffer)
            .map_err(|error| PackageError::Io(error.to_string()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    if should_cancel() {
        return Err(PackageError::Cancelled(format!("hashing cancelled: {}", path.display())));
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::{CapturePackage, PackageError, sha256_file, sha256_file_with_cancel};
    use serde_json::{Value, json};
    use std::cell::Cell;
    use std::fs;
    use tempfile::TempDir;

    const ABC_SHA256: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn manifest() -> Value {
        let clips: Vec<_> = (1..=3)
            .map(|index| {
                json!({
                    "clip_id": format!("clip-{index}"),
                    "relative_path": format!("clip-{index}.mp4"),
                    "sha256": ABC_SHA256,
                    "demo_sha256": ABC_SHA256,
                    "event_id": format!("event-{index}"),
                    "player_steamid64": "76561198000000001",
                    "map": "de_mirage",
                    "round": 4,
                    "utility_type": "smoke",
                    "start_tick": 1000 + index * 100,
                    "end_tick": 1100 + index * 100,
                    "camera_role": "projectile",
                    "duration_ms": 1000,
                    "width": 1920,
                    "height": 1080,
                    "fps_num": 60,
                    "fps_den": 1,
                    "audio": {"source": "hlae_wav", "channels": 2, "sample_rate": 48000, "verified": true},
                    "qc_status": "accepted",
                    "group_order": index
                })
            })
            .collect();
        json!({"schema_version": 1, "package_id": "pilot-001", "clips": clips})
    }

    fn package_with_manifest(value: &Value) -> TempDir {
        let dir = TempDir::new().unwrap();
        for index in 1..=3 {
            fs::write(dir.path().join(format!("clip-{index}.mp4")), b"abc").unwrap();
        }
        fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
        dir
    }

    #[test]
    fn loads_three_independent_clips_with_verified_bytes() {
        let dir = package_with_manifest(&manifest());
        let package = CapturePackage::load_and_validate(dir.path()).unwrap();
        assert_eq!(package.manifest.package_id, "pilot-001");
        assert_eq!(package.manifest.clips.len(), 3);
        assert_ne!(
            package.manifest.clips[0].clip_id,
            package.manifest.clips[1].clip_id
        );
        assert!(package.media_path(&package.manifest.clips[2]).is_file());
    }

    #[test]
    fn rejects_path_escape() {
        let mut value = manifest();
        value["clips"][0]["relative_path"] = json!("../outside.mp4");
        let dir = package_with_manifest(&value);
        assert!(matches!(
            CapturePackage::load_and_validate(dir.path()),
            Err(PackageError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_changed_media_bytes() {
        let dir = package_with_manifest(&manifest());
        fs::write(dir.path().join("clip-1.mp4"), b"changed").unwrap();
        assert!(matches!(
            CapturePackage::load_and_validate(dir.path()),
            Err(PackageError::HashMismatch(_))
        ));
    }

    #[test]
    fn rejects_missing_media() {
        let dir = package_with_manifest(&manifest());
        fs::remove_file(dir.path().join("clip-2.mp4")).unwrap();
        assert!(matches!(
            CapturePackage::load_and_validate(dir.path()),
            Err(PackageError::Io(_))
        ));
    }

    #[test]
    fn rejects_case_variant_duplicate_path() {
        let mut value = manifest();
        value["clips"][1]["relative_path"] = json!("CLIP-1.mp4");
        let dir = package_with_manifest(&value);
        assert!(matches!(
            CapturePackage::load_and_validate(dir.path()),
            Err(PackageError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_unknown_schema_version() {
        let mut value = manifest();
        value["schema_version"] = json!(2);
        let dir = package_with_manifest(&value);
        assert!(matches!(
            CapturePackage::load_and_validate(dir.path()),
            Err(PackageError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn rejects_accepted_clip_with_generated_silence() {
        let mut value = manifest();
        value["clips"][0]["audio"]["source"] = json!("generated_silence");
        let dir = package_with_manifest(&value);
        assert!(matches!(
            CapturePackage::load_and_validate(dir.path()),
            Err(PackageError::Invalid(_))
        ));
    }

    #[test]
    fn cancels_during_hash_without_changing_source() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("large.mp4");
        let bytes = vec![7_u8; 1024 * 1024];
        fs::write(&path, &bytes).unwrap();
        let calls = Cell::new(0);
        let result = sha256_file_with_cancel(&path, || {
            calls.set(calls.get() + 1);
            calls.get() >= 5
        });
        assert!(matches!(result, Err(PackageError::Cancelled(_))));
        assert!(calls.get() >= 5);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn cancels_package_validation_while_hashing_media() {
        let mut value = manifest();
        let dir = package_with_manifest(&value);
        let large = vec![9_u8; 1024 * 1024];
        let path = dir.path().join("clip-1.mp4");
        fs::write(&path, &large).unwrap();
        value["clips"][0]["sha256"] = json!(sha256_file(&path).unwrap());
        fs::write(dir.path().join("manifest.json"), serde_json::to_vec(&value).unwrap()).unwrap();
        let calls = Cell::new(0);
        let result = CapturePackage::load_and_validate_with_cancel(dir.path(), || {
            calls.set(calls.get() + 1);
            calls.get() >= 5
        });
        assert!(matches!(result, Err(PackageError::Cancelled(_))));
        assert_eq!(fs::read(&path).unwrap(), large);
    }
}
