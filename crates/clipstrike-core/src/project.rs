use crate::capture_package::{CapturePackage, CapturedAsset, PackageError};
use crate::timeline::{Clip, Timeline};
use crate::transitions::Transition;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaAsset {
    pub id: usize,
    pub name: String,
    pub uri: String,
    pub duration_ms: u64,
    #[serde(default)]
    pub audio_levels: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TransitionMaskAsset {
    pub id: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Marker {
    pub id: usize,
    pub time_ms: i64,
    pub label: String,
    pub color: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub enum MarkerToggleResult {
    Added(Marker),
    Removed(Marker),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum TitleTemplate {
    #[default]
    Centered,
    LowerThird,
    TopLeft,
    EndCredits,
    Subtitle,
    Watermark,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TitleAsset {
    pub id: usize,
    pub name: String,
    pub template: TitleTemplate,
    pub text: String,
    pub font_family: String,
    pub font_size: f32,
    pub color: [u8; 4],
    pub x: f32,
    pub y: f32,
    pub svg_content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum GeneratorType {
    #[default]
    SolidColor,
    LinearGradient,
    RadialGradient,
    TestPattern,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneratorAsset {
    pub id: usize,
    pub name: String,
    pub generator_type: GeneratorType,
    pub color1: [f32; 4], // RGBA
    pub color2: [f32; 4], // For gradients
    pub params: HashMap<String, f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub framerate: f64,
    pub master_volume: f32,
    pub tracks: Vec<Track>,
    pub assets: Vec<MediaAsset>,
    #[serde(default)]
    pub capture_sources: Vec<CapturedAsset>,
    pub titles: Vec<TitleAsset>,
    pub generators: Vec<GeneratorAsset>,
    pub transition_masks: Vec<TransitionMaskAsset>,
    pub clips: Vec<Clip>,
    pub transitions: Vec<Transition>,
    pub markers: Vec<Marker>,
    pub watermark_asset_id: Option<usize>,
    pub watermark_opacity: f32,
    pub watermark_scale: f32,
    pub watermark_pos_x: f32,
    pub watermark_pos_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Track {
    pub name: String,
    pub id: usize,
    pub visible: bool,
    pub audio_enabled: bool,
    pub solo: bool,
    pub locked: bool,
}

impl Track {
    pub fn new(id: usize, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            visible: true,
            audio_enabled: true,
            solo: false,
            locked: false,
        }
    }
}

impl Default for Track {
    fn default() -> Self {
        Self {
            name: String::new(),
            id: 0,
            visible: true,
            audio_enabled: true,
            solo: false,
            locked: false,
        }
    }
}

impl Project {
    pub fn new(name: &str, width: u32, height: u32, framerate: f64) -> Self {
        Self {
            name: name.to_string(),
            width,
            height,
            framerate,
            master_volume: 1.0,
            tracks: Vec::new(),
            assets: Vec::new(),
            capture_sources: Vec::new(),
            titles: Vec::new(),
            generators: Vec::new(),
            transition_masks: Vec::new(),
            clips: Vec::new(),
            transitions: Vec::new(),
            markers: Vec::new(),
            watermark_asset_id: None,
            watermark_opacity: 0.5,
            watermark_scale: 0.2,
            watermark_pos_x: 0.8,
            watermark_pos_y: 0.1,
        }
    }

    pub fn default_tracks(count: usize) -> Vec<Track> {
        (1..=count)
            .rev()
            .map(|i| Track::new(i, &format!("Track {}", i)))
            .collect()
    }

    pub fn import_capture_package(
        &mut self,
        package: &CapturePackage,
    ) -> Result<Vec<usize>, PackageError> {
        // Recheck the on-disk evidence immediately before linking it into a project.
        let verified = CapturePackage::load_and_validate(&package.root)?;
        if verified.manifest.clips.iter().any(|clip| clip.fps_num % clip.fps_den != 0) {
            return Err(PackageError::Invalid(
                "fractional source FPS is preserved in the package but not yet supported by the millisecond timeline".into(),
            ));
        }
        let mut ordered = verified.manifest.clips.clone();
        ordered.sort_by_key(|clip| clip.group_order);

        let mut next_asset_id = self.assets.iter().map(|asset| asset.id).max().unwrap_or(0);
        let mut next_clip_id = self.clips.iter().map(|clip| clip.id).max().unwrap_or(0);
        let mut start = self.clips.iter().map(|clip| clip.end).max().unwrap_or(0);
        let track_id = self.tracks.first().map(|track| track.id).unwrap_or(1);
        let mut assets = Vec::with_capacity(ordered.len());
        let mut clips = Vec::with_capacity(ordered.len());
        let mut sources = Vec::with_capacity(ordered.len());
        let mut imported_ids = Vec::with_capacity(ordered.len());

        for source in ordered {
            if self.capture_sources.iter().any(|existing| {
                existing.package_id == verified.manifest.package_id
                    && existing.clip.clip_id == source.clip_id
            }) {
                return Err(PackageError::Invalid(format!(
                    "capture clip already imported: {}", source.clip_id
                )));
            }
            let duration = i64::try_from(source.duration_ms)
                .map_err(|_| PackageError::Invalid("clip duration exceeds timeline range".into()))?;
            let end = start.checked_add(duration)
                .ok_or_else(|| PackageError::Invalid("timeline duration overflow".into()))?;
            next_asset_id = next_asset_id.checked_add(1)
                .ok_or_else(|| PackageError::Invalid("asset ID overflow".into()))?;
            next_clip_id = next_clip_id.checked_add(1)
                .ok_or_else(|| PackageError::Invalid("clip ID overflow".into()))?;
            let uri = url::Url::from_file_path(verified.media_path(&source))
                .map_err(|_| PackageError::Invalid(format!("cannot encode media path: {}", source.relative_path)))?
                .to_string();
            assets.push(MediaAsset {
                id: next_asset_id,
                name: source.clip_id.clone(),
                uri,
                duration_ms: source.duration_ms,
                audio_levels: Vec::new(),
            });
            clips.push(Clip::new(next_clip_id, next_asset_id, track_id, start, end, 0, duration));
            sources.push(CapturedAsset {
                asset_id: next_asset_id,
                package_id: verified.manifest.package_id.clone(),
                package_path: Some(verified.root.to_string_lossy().into_owned()),
                media_validation: verified.manifest.media_validation,
                clip: source,
            });
            imported_ids.push(next_clip_id);
            start = end;
        }

        if self.tracks.is_empty() {
            self.add_track();
        }
        self.assets.extend(assets);
        self.clips.extend(clips);
        self.capture_sources.extend(sources);
        Ok(imported_ids)
    }

    /// Store a package location relative to the project when the paths share a volume.
    /// The complete package is revalidated before any media URI is changed.
    pub fn relink_capture_package(
        &mut self,
        project_file: &Path,
        package_id: &str,
        package_root: &Path,
    ) -> Result<(), PackageError> {
        let package = CapturePackage::load_and_validate(package_root)?;
        if package.manifest.package_id != package_id {
            return Err(PackageError::Invalid("capture package ID does not match".into()));
        }
        let parent = project_file.parent().unwrap_or_else(|| Path::new("."))
            .canonicalize().map_err(|error| PackageError::Io(error.to_string()))?;
        let path_hint = relative_path(&parent, &package.root)
            .unwrap_or_else(|| package.root.clone()).to_string_lossy().into_owned();
        for source in self.capture_sources.iter().filter(|source| source.package_id == package_id) {
            let matching = package.manifest.clips.iter().any(|clip| {
                clip == &source.clip
            });
            if !matching {
                return Err(PackageError::Invalid(format!("capture source missing or changed: {}", source.clip.clip_id)));
            }
        }
        for source in self.capture_sources.iter_mut().filter(|source| source.package_id == package_id) {
            source.package_path = Some(path_hint.clone());
            if let Some(asset) = self.assets.iter_mut().find(|asset| asset.id == source.asset_id) {
                asset.uri = url::Url::from_file_path(package.media_path(&source.clip))
                    .map_err(|_| PackageError::Invalid("cannot encode capture media path".into()))?
                    .to_string();
            }
        }
        Ok(())
    }

    /// Resolve capture media from the stored package location. Missing or changed packages
    /// become explicit offline URIs rather than silently using a stale absolute source.
    pub fn relink_capture_media(&mut self, project_file: &Path) -> Vec<String> {
        let parent = project_file.parent().unwrap_or_else(|| Path::new("."));
        let mut packages: HashMap<PathBuf, Result<CapturePackage, PackageError>> = HashMap::new();
        let mut errors = Vec::new();
        for source in &self.capture_sources {
            let Some(hint) = &source.package_path else {
                if let Some(asset) = self.assets.iter_mut().find(|asset| asset.id == source.asset_id) {
                    asset.uri = format!("capture-offline://{}/{}", source.package_id, source.clip.clip_id);
                }
                errors.push(format!("capture media unavailable: {} / {}: package location is missing", source.package_id, source.clip.clip_id));
                continue;
            };
            let stored = Path::new(hint);
            let root = if stored.is_absolute() { stored.to_path_buf() } else { parent.join(stored) };
            let package = packages.entry(root.clone())
                .or_insert_with(|| CapturePackage::load_manifest_structure(&root));
            let resolved = package.as_ref().map_err(|error| error.to_string()).and_then(|package| {
                if package.manifest.package_id != source.package_id
                    || !package.manifest.clips.iter().any(|clip| clip == &source.clip) {
                    return Err("stored capture metadata differs from package".into());
                }
                package.validate_media_clip_with_cancel(&source.clip, || false)
                    .map_err(|error| error.to_string())
            }).and_then(|path| {
                url::Url::from_file_path(path)
                    .map_err(|_| "cannot encode capture media path".to_string())
            });
            if let Some(asset) = self.assets.iter_mut().find(|asset| asset.id == source.asset_id) {
                match resolved {
                    Ok(uri) => { asset.uri = uri.to_string(); continue; }
                    Err(reason) => {
                        asset.uri = format!("capture-offline://{}/{}", source.package_id, source.clip.clip_id);
                        errors.push(format!("capture media unavailable: {} / {}: {reason}", source.package_id, source.clip.clip_id));
                    }
                }
            }
        }
        errors
    }

    pub fn capture_timeline(&mut self, timeline: &Timeline) {
        self.clips = timeline.clips.clone();
        self.transitions = timeline.transitions.clone();
    }

    pub fn to_timeline(&self) -> Timeline {
        Timeline::from_parts(self.clips.clone(), self.transitions.clone())
    }

    pub fn add_track(&mut self) -> usize {
        let next_label = self.tracks.len() + 1;
        self.tracks
            .insert(0, Track::new(next_label, &format!("Track {}", next_label)));
        self.renumber_tracks();
        self.tracks[0].id
    }

    pub fn remove_track(&mut self, track_id: usize) -> Result<Track, String> {
        if self.tracks.len() <= 1 {
            return Err("Cannot remove the last track".to_string());
        }

        let track_index = self
            .track_index(track_id)
            .ok_or_else(|| format!("Track {track_id} not found"))?;
        let removed = self.tracks.remove(track_index);
        self.clips.retain(|clip| clip.track_id != track_id);
        self.transitions
            .retain(|transition| transition.track_id != track_id);
        self.renumber_tracks();
        Ok(removed)
    }

    pub fn rename_track(
        &mut self,
        track_id: usize,
        new_name: impl AsRef<str>,
    ) -> Result<(), String> {
        let trimmed = new_name.as_ref().trim();
        if trimmed.is_empty() {
            return Err("Track name cannot be empty".to_string());
        }

        let track = self
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| format!("Track {track_id} not found"))?;
        track.name = trimmed.to_string();
        Ok(())
    }

    pub fn reorder_tracks(&mut self, from_index: usize, to_index: usize) -> Result<(), String> {
        if from_index >= self.tracks.len() || to_index >= self.tracks.len() {
            return Err("Track reorder index out of range".to_string());
        }
        if from_index == to_index {
            return Ok(());
        }

        let track = self.tracks.remove(from_index);
        self.tracks.insert(to_index, track);
        self.renumber_tracks();
        Ok(())
    }

    pub fn track_index(&self, track_id: usize) -> Option<usize> {
        self.tracks.iter().position(|track| track.id == track_id)
    }

    fn renumber_tracks(&mut self) {
        let track_count = self.tracks.len();
        let mut id_map = HashMap::with_capacity(track_count);

        for (index, track) in self.tracks.iter_mut().enumerate() {
            let old_id = track.id;
            let new_id = track_count - index;
            track.id = new_id;
            id_map.insert(old_id, new_id);
        }

        for clip in &mut self.clips {
            if let Some(new_id) = id_map.get(&clip.track_id) {
                clip.track_id = *new_id;
            }
        }

        for transition in &mut self.transitions {
            if let Some(new_id) = id_map.get(&transition.track_id) {
                transition.track_id = *new_id;
            }
        }
    }

    pub fn toggle_marker(&mut self, time_ms: i64) -> MarkerToggleResult {
        if let Some(pos) = self
            .markers
            .iter()
            .position(|marker| marker.time_ms == time_ms)
        {
            return MarkerToggleResult::Removed(self.markers.remove(pos));
        }

        let marker = self.add_marker(time_ms);
        MarkerToggleResult::Added(marker)
    }

    pub fn add_marker(&mut self, time_ms: i64) -> Marker {
        let marker = Marker {
            id: rand::random::<usize>(),
            time_ms,
            label: format!("Marker {}", self.markers.len() + 1),
            color: [255, 200, 0, 255],
        };
        self.markers.push(marker.clone());
        self.markers.sort_by_key(|marker| marker.time_ms);
        marker
    }

    pub fn previous_marker(&self, time_ms: i64) -> Option<&Marker> {
        self.markers
            .iter()
            .filter(|marker| marker.time_ms < time_ms)
            .max_by_key(|marker| marker.time_ms)
    }

    pub fn next_marker(&self, time_ms: i64) -> Option<&Marker> {
        self.markers
            .iter()
            .filter(|marker| marker.time_ms > time_ms)
            .min_by_key(|marker| marker.time_ms)
    }

    pub fn is_track_audible(&self, track_id: usize) -> bool {
        let any_solo = self.tracks.iter().any(|track| track.solo);
        self.tracks
            .iter()
            .find(|track| track.id == track_id)
            .map(|track| {
                if any_solo {
                    track.solo
                } else {
                    track.audio_enabled
                }
            })
            .unwrap_or(false)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        self.save_with_before_replace(path.as_ref(), || Ok(()))
    }

    fn save_with_before_replace(
        &self,
        path: &Path,
        before_replace: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut pending = tempfile::NamedTempFile::new_in(parent)?;
        pending.write_all(&self.to_json_bytes()?)?;
        pending.as_file().sync_all()?;
        if path.is_file() {
            let mut backup = tempfile::NamedTempFile::new_in(parent)?;
            io::copy(&mut std::fs::File::open(path)?, &mut backup)?;
            backup.as_file().sync_all()?;
            backup.persist(Self::backup_path(path)).map_err(|error| error.error)?;
        }
        before_replace()?;
        pending.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    pub fn backup_path(path: &Path) -> PathBuf {
        let mut value = path.as_os_str().to_os_string();
        value.push(".bak");
        PathBuf::from(value)
    }

    pub fn load_backup(path: &Path) -> io::Result<Self> {
        Self::load(Self::backup_path(path))
    }

    pub fn load<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        Self::load_with_capture_warnings(path).map(|(project, _)| project)
    }

    /// Open editorial state even when linked source media is unavailable, while
    /// returning the reason each capture asset was marked offline.
    pub fn load_with_capture_warnings<P: AsRef<Path>>(path: P) -> std::io::Result<(Self, Vec<String>)> {
        let content = std::fs::read(&path)?;
        let mut project = Self::from_json_bytes(&content)?;
        let warnings = project.relink_capture_media(path.as_ref());
        Ok((project, warnings))
    }

    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    if from_parts.first() != to_parts.first() { return None; }
    let common = from_parts.iter().zip(&to_parts).take_while(|(a, b)| a == b).count();
    let mut relative = PathBuf::new();
    for part in &from_parts[common..] {
        if !matches!(part, Component::Normal(_)) { return None; }
        relative.push("..");
    }
    for part in &to_parts[common..] { relative.push(part.as_os_str()); }
    if relative.as_os_str().is_empty() { relative.push("."); }
    Some(relative)
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, PartialEq)]
pub enum EngineCommand {
    Play,
    Pause,
    Seek(f64),
    LoadProject(String),
    SaveProject(String),
}

#[cfg(not(target_arch = "wasm32"))]
pub struct EngineChannels {
    pub sender: mpsc::Sender<EngineCommand>,
    pub receiver: mpsc::Receiver<EngineCommand>,
}

#[cfg(not(target_arch = "wasm32"))]
impl EngineChannels {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(capacity);
        Self { sender, receiver }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframes::InterpolationMode;
    use crate::transitions::TransitionType;

    #[test]
    fn track_audio_uses_mute_when_no_track_is_soloed() {
        let mut proj = Project::new("Audio", 1920, 1080, 60.0);
        proj.tracks = Project::default_tracks(2);
        proj.tracks[0].audio_enabled = false;

        assert!(!proj.is_track_audible(proj.tracks[0].id));
        assert!(proj.is_track_audible(proj.tracks[1].id));
    }

    #[test]
    fn track_audio_solo_overrides_regular_mute_state() {
        let mut proj = Project::new("Audio", 1920, 1080, 60.0);
        proj.tracks = Project::default_tracks(3);
        proj.tracks[2].solo = true;
        proj.tracks[2].audio_enabled = false;

        assert!(proj.is_track_audible(proj.tracks[2].id));
        assert!(!proj.is_track_audible(proj.tracks[0].id));
        assert!(!proj.is_track_audible(proj.tracks[1].id));
    }
fn make_clip(id: usize, track_id: usize, start: i64, end: i64) -> Clip {
    Clip::new(id, id, track_id, start, end, 0, end - start)
}

    #[test]
    fn test_project_serialization_round_trips_runtime_state() {
        let mut proj = Project::new("Test Project", 1920, 1080, 60.0);
        proj.tracks = Project::default_tracks(2);
        proj.tracks[0].solo = true;
        proj.assets.push(MediaAsset {
            id: 1,
            name: "clip.mp4".to_string(),
            uri: "file:///clip.mp4".to_string(),
            duration_ms: 1_000,
            audio_levels: vec![0.25, 0.75],
        });
        proj.transition_masks.push(TransitionMaskAsset {
            id: 42,
            name: "Diagonal".to_string(),
            width: 2,
            height: 2,
            pixels: vec![0, 255, 255, 0],
        });
        proj.clips.push(make_clip(1, 1, 0, 1_000));
        let mut transition = Transition::new(99, 1, 250, 750, TransitionType::CustomMask);
        transition.custom_mask_id = Some(42);
        transition
            .speed
            .insert_keyframe(500, 2.0, InterpolationMode::Linear);
        transition
            .softness
            .insert_keyframe(500, 0.2, InterpolationMode::Linear);
        proj.transitions.push(transition);
        proj.markers.push(Marker {
            id: 7,
            time_ms: 500,
            label: "Beat".to_string(),
            color: [255, 200, 0, 255],
        });

        let temp_dir = std::env::temp_dir();
        let path = temp_dir.join("test_project.cstrike");

        proj.save(&path).expect("Failed to save project");
        let loaded = Project::load(&path).expect("Failed to load project");

        assert_eq!(proj, loaded);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_to_timeline_rehydrates_indices() {
        let mut proj = Project::new("Test Project", 1920, 1080, 60.0);
        proj.transition_masks.push(TransitionMaskAsset {
            id: 7,
            name: "Mask".to_string(),
            width: 1,
            height: 1,
            pixels: vec![255],
        });
        proj.clips.push(make_clip(1, 1, 100, 300));
        let mut transition = Transition::new(1, 1, 150, 250, TransitionType::CustomMask);
        transition.custom_mask_id = Some(7);
        transition
            .speed
            .insert_keyframe(200, 1.5, InterpolationMode::Linear);
        proj.transitions.push(transition);

        let timeline = proj.to_timeline();

        assert_eq!(timeline.get_clips_at_time(200).len(), 1);
        assert_eq!(timeline.transitions.len(), 1);
        assert_eq!(timeline.transitions[0].start_ms, 150);
    }

    #[tokio::test]
    async fn test_engine_channels() {
        let mut channels = EngineChannels::new(32);

        channels.sender.send(EngineCommand::Play).await.unwrap();
        channels
            .sender
            .send(EngineCommand::Seek(1.5))
            .await
            .unwrap();

        let cmd1 = channels.receiver.recv().await.unwrap();
        assert_eq!(cmd1, EngineCommand::Play);

        let cmd2 = channels.receiver.recv().await.unwrap();
        assert_eq!(cmd2, EngineCommand::Seek(1.5));
    }

    #[test]
    fn add_and_remove_track_updates_track_ids_and_clips() {
        let mut project = Project::new("Test Project", 1920, 1080, 60.0);
        project.tracks = Project::default_tracks(2);
        project.clips.push(make_clip(1, 2, 0, 100));
        project.clips.push(make_clip(2, 1, 100, 200));
        project
            .transitions
            .push(Transition::new(1, 2, 10, 90, TransitionType::Crossfade));

        let new_id = project.add_track();
        assert_eq!(new_id, 3);
        assert_eq!(project.tracks[0].name, "Track 3");
        assert_eq!(
            project
                .tracks
                .iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );

        let removed = project.remove_track(2).expect("track should be removed");
        assert_eq!(removed.name, "Track 2");
        assert_eq!(project.tracks.len(), 2);
        assert_eq!(project.clips.len(), 1);
        assert_eq!(project.transitions.len(), 0);
        assert_eq!(
            project
                .clips
                .iter()
                .map(|clip| clip.track_id)
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert!(project.remove_track(1).is_ok());
        assert!(project.remove_track(2).is_err());
    }

    #[test]
    fn reorder_tracks_reassigns_track_ids_and_z_order() {
        let mut project = Project::new("Test Project", 1920, 1080, 60.0);
        project.tracks = Project::default_tracks(3);
        project.clips.push(make_clip(1, 3, 0, 100));
        project.clips.push(make_clip(2, 1, 100, 200));
        project
            .transitions
            .push(Transition::new(1, 3, 10, 90, TransitionType::Crossfade));

        project
            .reorder_tracks(0, 2)
            .expect("reorder should succeed");

        assert_eq!(
            project
                .tracks
                .iter()
                .map(|track| track.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Track 2", "Track 1", "Track 3"]
        );
        assert_eq!(
            project
                .tracks
                .iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );
        assert_eq!(
            project
                .clips
                .iter()
                .map(|clip| clip.track_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(project.transitions[0].track_id, 1);
    }

    #[test]
    fn rename_track_updates_name_and_rejects_blank_names() {
        let mut project = Project::new("Test Project", 1920, 1080, 60.0);
        project.tracks = Project::default_tracks(2);

        project
            .rename_track(2, "Primary Video")
            .expect("rename should succeed");
        assert_eq!(project.tracks[0].name, "Primary Video");
        assert!(project.rename_track(2, "   ").is_err());
    }

    #[test]
    fn toggle_marker_adds_sorted_and_removes_same_timestamp() {
        let mut project = Project::new("Test Project", 1920, 1080, 60.0);

        let added = project.toggle_marker(1_000);
        assert!(matches!(added, MarkerToggleResult::Added(_)));

        project.toggle_marker(250);
        project.toggle_marker(750);

        let times: Vec<i64> = project
            .markers
            .iter()
            .map(|marker| marker.time_ms)
            .collect();
        assert_eq!(times, vec![250, 750, 1_000]);

        let removed = project.toggle_marker(750);
        assert!(matches!(removed, MarkerToggleResult::Removed(marker) if marker.time_ms == 750));

        let times: Vec<i64> = project
            .markers
            .iter()
            .map(|marker| marker.time_ms)
            .collect();
        assert_eq!(times, vec![250, 1_000]);
    }

    #[test]
    fn add_marker_always_inserts_and_keeps_timeline_sorted() {
        let mut project = Project::new("Test Project", 1920, 1080, 60.0);

        let first = project.add_marker(1_000);
        let second = project.add_marker(250);

        assert_eq!(first.time_ms, 1_000);
        assert_eq!(second.time_ms, 250);
        assert_eq!(
            project
                .markers
                .iter()
                .map(|marker| marker.time_ms)
                .collect::<Vec<_>>(),
            vec![250, 1_000]
        );
    }

    #[test]
    fn marker_navigation_finds_previous_and_next() {
        let mut project = Project::new("Test Project", 1920, 1080, 60.0);
        project.markers = vec![
            Marker {
                id: 1,
                time_ms: 100,
                label: "A".into(),
                color: [0, 0, 0, 255],
            },
            Marker {
                id: 2,
                time_ms: 300,
                label: "B".into(),
                color: [0, 0, 0, 255],
            },
            Marker {
                id: 3,
                time_ms: 700,
                label: "C".into(),
                color: [0, 0, 0, 255],
            },
        ];

        assert_eq!(project.previous_marker(50), None);
        assert_eq!(project.next_marker(800), None);
        assert_eq!(
            project.previous_marker(500).map(|marker| marker.id),
            Some(2)
        );
        assert_eq!(project.next_marker(500).map(|marker| marker.id), Some(3));
    }

    #[test]
    fn test_track_defaults() {
        let track = Track::default();
        assert_eq!(track.name, "");
        assert_eq!(track.id, 0);
        assert!(track.visible);
        assert!(track.audio_enabled);
        assert!(!track.solo);
        assert!(!track.locked);

        let named_track = Track::new(5, "B-Roll");
        assert_eq!(named_track.id, 5);
        assert_eq!(named_track.name, "B-Roll");
        assert!(named_track.visible);
    }

    #[test]
    fn test_project_capture_timeline() {
        let mut proj = Project::new("Capture Test", 1920, 1080, 60.0);
        let mut timeline = Timeline::new();
        timeline.add_clip(make_clip(1, 1, 0, 100));
        timeline
            .transitions
            .push(Transition::new(1, 1, 50, 100, TransitionType::Crossfade));

        proj.capture_timeline(&timeline);

        assert_eq!(proj.clips.len(), 1);
        assert_eq!(proj.clips[0].id, 1);
        assert_eq!(proj.transitions.len(), 1);
        assert_eq!(proj.transitions[0].id, 1);
    }

    #[test]
    fn project_json_bytes_round_trip_preserves_state() {
        let mut proj = Project::new("Bytes", 1280, 720, 30.0);
        proj.tracks = Project::default_tracks(2);
        proj.assets.push(MediaAsset {
            id: 9,
            name: "Import.mp4".to_string(),
            uri: "browser://Import.mp4".to_string(),
            duration_ms: 1_500,
            audio_levels: vec![0.1, 0.2, 0.3],
        });
        proj.markers.push(Marker {
            id: 7,
            time_ms: 250,
            label: "Cue".to_string(),
            color: [255, 0, 0, 255],
        });

        let bytes = proj.to_json_bytes().expect("project should serialize");
        let loaded = Project::from_json_bytes(&bytes).expect("project should deserialize");

        assert_eq!(proj, loaded);
    }

    #[test]
    fn interrupted_save_keeps_last_good_project_and_backup() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("edit.cstrike");
        let original = Project::new("Original", 1920, 1080, 60.0);
        original.save(&path).unwrap();
        let changed = Project::new("Changed", 1920, 1080, 60.0);
        let failure = changed.save_with_before_replace(&path, || {
            Err(io::Error::other("simulated interruption before replacement"))
        });
        assert!(failure.is_err());
        assert_eq!(Project::load(&path).unwrap().name, "Original");
        changed.save(&path).unwrap();
        assert_eq!(Project::load(&path).unwrap().name, "Changed");
        assert_eq!(Project::load_backup(&path).unwrap().name, "Original");
        std::fs::write(&path, b"invalid JSON").unwrap();
        assert!(Project::load(&path).is_err());
        assert_eq!(Project::load_backup(&path).unwrap().name, "Original");
    }

    #[test]
    fn capture_package_import_keeps_three_editable_sources_after_reopen() {
        use crate::capture_package::CapturePackage;
        use serde_json::json;
        use std::fs;

        let dir = tempfile::TempDir::new().unwrap();
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let clips: Vec<_> = (1..=3)
            .map(|order| {
                fs::write(dir.path().join(format!("clip-{order}.mp4")), b"abc").unwrap();
                json!({
                    "clip_id": format!("clip-{order}"),
                    "relative_path": format!("clip-{order}.mp4"),
                    "sha256": hash, "demo_sha256": hash,
                    "event_id": format!("event-{order}"),
                    "player_steamid64": "76561198000000001",
                    "map": "de_mirage", "round": 4, "utility_type": "smoke",
                    "start_tick": 1000 + order * 100, "end_tick": 1100 + order * 100,
                    "camera_role": "projectile", "duration_ms": 1000,
                    "width": 1920, "height": 1080, "fps_num": 60, "fps_den": 1,
                    "audio": {"source": "hlae_wav", "channels": 2, "sample_rate": 48000, "verified": true},
                    "qc_status": "accepted", "group_order": order
                })
            })
            .collect();
        fs::write(dir.path().join("manifest.json"),
            serde_json::to_vec(&json!({"schema_version": 1, "package_id": "pilot-001", "clips": clips})).unwrap()).unwrap();
        let package = CapturePackage::load_and_validate(dir.path()).unwrap();

        let mut project = Project::new("Pilot", 1920, 1080, 60.0);
        let imported_ids = project.import_capture_package(&package).unwrap();
        assert_eq!(imported_ids.len(), 3);
        assert_eq!(project.assets.len(), 3);
        assert_eq!(project.clips.len(), 3);
        assert_eq!(project.capture_sources.len(), 3);
        assert_ne!(project.clips[0].asset_id, project.clips[1].asset_id);

        project.clips[1].start = 4_000;
        project.clips[1].end = 5_000;
        let project_path = dir.path().join("pilot.cstrike");
        project.save(&project_path).unwrap();
        let reopened = Project::load(&project_path).unwrap();
        assert_eq!(reopened.clips[1].start, 4_000);
        assert_eq!(reopened.capture_sources[1].clip.clip_id, "clip-2");
        assert_eq!(reopened.capture_sources[1].clip.sha256, hash);
        assert_eq!(reopened.assets.len(), 3);
        assert!(reopened.assets.iter().all(|asset| asset.uri.starts_with("file:///")));
    }
}
