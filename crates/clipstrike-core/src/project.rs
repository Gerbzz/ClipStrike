use crate::timeline::{Clip, Timeline};
use crate::transitions::Transition;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
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
        std::fs::write(path, self.to_json_bytes()?)?;
        Ok(())
    }

    pub fn load<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let content = std::fs::read(path)?;
        Ok(Self::from_json_bytes(&content)?)
    }

    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec_pretty(self)
    }

    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
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
    use crate::keyframes::KeyframedProperty;
    use crate::timeline::BlendMode;
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
        Clip {
            id,
            asset_id: id,
            track_id,
            blend_mode: BlendMode::Normal,
            start,
            end,
            brightness: KeyframedProperty::new(1.0),
            contrast: KeyframedProperty::new(1.0),
            saturation: KeyframedProperty::new(1.0),
            grayscale: KeyframedProperty::new(0.0),
            blur_radius: KeyframedProperty::new(0.0),
            volume: KeyframedProperty::new(1.0),
            pos_x: KeyframedProperty::new(0.0),
            pos_y: KeyframedProperty::new(0.0),
            scale_x: KeyframedProperty::new(1.0),
            scale_y: KeyframedProperty::new(1.0),
            rotation: KeyframedProperty::new(0.0),
            opacity: KeyframedProperty::new(1.0),
            speed: KeyframedProperty::new(1.0),
            pixelate_size: KeyframedProperty::new(1.0),
            gamma: KeyframedProperty::new(1.0),
            hue: KeyframedProperty::new(0.0),
            color_tint_r: KeyframedProperty::new(1.0),
            color_tint_g: KeyframedProperty::new(1.0),
            color_tint_b: KeyframedProperty::new(1.0),
            chroma_key_threshold: KeyframedProperty::new(0.0),
            chroma_key_fuzziness: KeyframedProperty::new(0.1),
            chroma_key_r: KeyframedProperty::new(0.0),
            chroma_key_g: KeyframedProperty::new(1.0),
            chroma_key_b: KeyframedProperty::new(0.0),
            lut_enabled: KeyframedProperty::new(0.0),
            lut_path: None,
            lut_size: 0,
            reverse: false,
            freeze_frame_ms: None,
            audio_enabled: true,
            video_enabled: true,
            tracking_data: Vec::new(),
            detections: Vec::new(),
            effects: Vec::new(),
            stabilization_enabled: false,
            motion_tracking_enabled: false,
            object_detection_enabled: false,
            stabilization_strength: KeyframedProperty::new(0.0),
        }
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
}
