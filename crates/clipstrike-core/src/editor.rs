use std::path::{Path, PathBuf};

use crate::commands::{
    AddClipCommand, MoveClipCommand, RemoveClipCommand, TrimClipCommand, UndoStack,
};
use crate::effects::MotionTrack;
use crate::keyframes::InterpolationMode;
use crate::project::{Marker, MarkerToggleResult, MediaAsset, Project, TransitionMaskAsset};
use crate::timeline::{Clip, Timeline};
use crate::transitions::{TransitionType, fade_in_points, fade_out_points};

pub struct EditorSession {
    pub project: Project,
    pub timeline: Timeline,
    pub undo_stack: UndoStack,
    pub current_project_path: Option<PathBuf>,
}

impl EditorSession {
    pub fn new(project: Project) -> Self {
        let timeline = project.to_timeline();
        Self {
            project,
            timeline,
            undo_stack: UndoStack::new(),
            current_project_path: None,
        }
    }

    pub fn with_default_project(
        name: &str,
        width: u32,
        height: u32,
        framerate: f64,
        track_count: usize,
    ) -> Self {
        let mut project = Project::new(name, width, height, framerate);
        project.tracks = Project::default_tracks(track_count.max(1));
        Self::new(project)
    }

    pub fn open_from_path(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let project = Project::load(&path).map_err(|err| err.to_string())?;
        let mut session = Self::new(project);
        session.current_project_path = Some(path);
        Ok(session)
    }

    pub fn snapshot_project(&self) -> Project {
        let mut project = self.project.clone();
        project.capture_timeline(&self.timeline);
        project
    }

    pub fn sync_project_from_timeline(&mut self) {
        self.project.capture_timeline(&self.timeline);
    }

    pub fn save_to_path(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref().to_path_buf();
        let project = self.snapshot_project();
        project.save(&path).map_err(|err| err.to_string())?;
        self.project = project;
        self.current_project_path = Some(path);
        Ok(())
    }

    pub fn timeline_end_ms(&self) -> i64 {
        self.timeline
            .clips
            .iter()
            .map(|clip| clip.end)
            .max()
            .unwrap_or(0)
    }

    pub fn frame_duration_ms(&self) -> i64 {
        (1000.0 / self.project.framerate.max(1.0)).round() as i64
    }

    pub fn add_asset(
        &mut self,
        name: impl Into<String>,
        uri: impl Into<String>,
        duration_ms: u64,
    ) -> usize {
        let asset = MediaAsset {
            id: rand::random::<usize>(),
            name: name.into(),
            uri: uri.into(),
            duration_ms,
            audio_levels: Vec::new(),
        };
        let id = asset.id;
        self.project.assets.push(asset);
        id
    }
    pub fn build_clip(&self, asset_id: usize, track_id: usize, start: i64, end: i64) -> Clip {
        Clip::new(
            rand::random::<usize>(),
            asset_id,
            track_id,
            start,
            end,
            0,
            end - start,
        )
    }
    pub fn add_clip(&mut self, clip: Clip) -> usize {
        let clip_id = clip.id;
        self.undo_stack.push(
            Box::new(AddClipCommand { clip }),
            &mut self.project,
            &mut self.timeline,
        );
        clip_id
    }

    pub fn remove_clip(&mut self, clip_id: usize) -> Result<(), String> {
        let clip = self
            .timeline
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .cloned()
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;
        self.undo_stack.push(
            Box::new(RemoveClipCommand { clip }),
            &mut self.project,
            &mut self.timeline,
        );
        Ok(())
    }

    pub fn move_clip(
        &mut self,
        clip_id: usize,
        new_start: i64,
        new_end: i64,
    ) -> Result<(), String> {
        let clip = self
            .timeline
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .cloned()
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;
        self.undo_stack.push(
            Box::new(MoveClipCommand {
                clip_id,
                old_start: clip.start,
                old_end: clip.end,
                new_start,
                new_end,
                old_track_id: clip.track_id,
                new_track_id: clip.track_id,
            }),
            &mut self.project,
            &mut self.timeline,
        );
        Ok(())
    }

    pub fn move_clip_to_track(
        &mut self,
        clip_id: usize,
        new_track_id: usize,
    ) -> Result<(), String> {
        let clip = self
            .timeline
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .cloned()
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;
        self.undo_stack.push(
            Box::new(MoveClipCommand {
                clip_id,
                old_start: clip.start,
                old_end: clip.end,
                new_start: clip.start,
                new_end: clip.end,
                old_track_id: clip.track_id,
                new_track_id,
            }),
            &mut self.project,
            &mut self.timeline,
        );
        Ok(())
    }

    pub fn trim_clip(
        &mut self,
        clip_id: usize,
        new_start: i64,
        new_end: i64,
    ) -> Result<(), String> {
        let clip = self
            .timeline
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .cloned()
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;
        self.undo_stack.push(
            Box::new(TrimClipCommand {
                clip_id,
                old_start: clip.start,
                old_end: clip.end,
                new_start,
                new_end,
            }),
            &mut self.project,
            &mut self.timeline,
        );
        Ok(())
    }

    pub fn undo(&mut self) {
        self.undo_stack.undo(&mut self.project, &mut self.timeline);
    }

    pub fn redo(&mut self) {
        self.undo_stack.redo(&mut self.project, &mut self.timeline);
    }

    pub fn apply_track_mutation<F>(&mut self, mutate: F) -> Result<(), String>
    where
        F: FnOnce(&mut Project) -> Result<(), String>,
    {
        self.project.capture_timeline(&self.timeline);
        mutate(&mut self.project)?;
        self.timeline = self.project.to_timeline();
        Ok(())
    }

    pub fn toggle_marker(&mut self, time_ms: i64) -> MarkerToggleResult {
        self.project.toggle_marker(time_ms)
    }

    pub fn previous_marker(&self, time_ms: i64) -> Option<&Marker> {
        self.project.previous_marker(time_ms)
    }

    pub fn next_marker(&self, time_ms: i64) -> Option<&Marker> {
        self.project.next_marker(time_ms)
    }

    pub fn apply_fade_in(&mut self, clip_id: usize) -> Result<(), String> {
        let clip = self
            .timeline
            .clips
            .iter_mut()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;
        for (time_ms, value) in fade_in_points(clip.start, clip.end) {
            clip.opacity
                .insert_keyframe(time_ms, value, InterpolationMode::Linear);
        }
        Ok(())
    }

    pub fn apply_fade_out(&mut self, clip_id: usize) -> Result<(), String> {
        let clip = self
            .timeline
            .clips
            .iter_mut()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;
        for (time_ms, value) in fade_out_points(clip.start, clip.end) {
            clip.opacity
                .insert_keyframe(time_ms, value, InterpolationMode::Linear);
        }
        Ok(())
    }

    pub fn set_clip_speed_keyframe(
        &mut self,
        clip_id: usize,
        time_ms: i64,
        value: f32,
        mode: InterpolationMode,
    ) -> Result<(), String> {
        let clip = self
            .timeline
            .clips
            .iter_mut()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;
        clip.speed.insert_keyframe(time_ms, value, mode);
        Ok(())
    }

    pub fn set_clip_reverse(&mut self, clip_id: usize, reverse: bool) -> Result<(), String> {
        let clip = self
            .timeline
            .clips
            .iter_mut()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;
        clip.reverse = reverse;
        Ok(())
    }

    pub fn freeze_clip_at_source_time(
        &mut self,
        clip_id: usize,
        source_time_ms: Option<i64>,
    ) -> Result<(), String> {
        let clip = self
            .timeline
            .clips
            .iter_mut()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;
        clip.freeze_frame_ms = source_time_ms.map(|value| value.max(0));
        Ok(())
    }

    pub fn apply_motion_track(&mut self, clip_id: usize, track: MotionTrack) -> Result<(), String> {
        let clip = self
            .timeline
            .clips
            .iter_mut()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;

        clip.motion_tracking_enabled = true;

        for point in &track.points {
            clip.pos_x
                .insert_keyframe(point.time_ms, point.x, InterpolationMode::Linear);
            clip.pos_y
                .insert_keyframe(point.time_ms, point.y, InterpolationMode::Linear);
        }

        clip.tracking_data.push(track);

        Ok(())
    }

    pub fn clip_source_time_at(
        &self,
        clip_id: usize,
        timeline_time_ms: i64,
    ) -> Result<i64, String> {
        let clip = self
            .timeline
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .ok_or_else(|| format!("Clip {clip_id} not found"))?;
        Ok(clip.source_time_at(timeline_time_ms))
    }

    pub fn import_transition_mask(
        &mut self,
        name: impl Into<String>,
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    ) -> Result<usize, String> {
        let expected_len = (width as usize).saturating_mul(height as usize);
        if width == 0 || height == 0 {
            return Err("Transition mask dimensions must be non-zero".to_string());
        }
        if pixels.len() != expected_len {
            return Err(format!(
                "Transition mask pixel data length {} did not match {}x{}",
                pixels.len(),
                width,
                height
            ));
        }

        let asset = TransitionMaskAsset {
            id: rand::random::<usize>(),
            name: name.into(),
            width,
            height,
            pixels,
        };
        let id = asset.id;
        self.project.transition_masks.push(asset);
        Ok(id)
    }

    pub fn set_transition_type(
        &mut self,
        transition_id: usize,
        transition_type: TransitionType,
        custom_mask_id: Option<usize>,
    ) -> Result<(), String> {
        let transition = self
            .timeline
            .transitions
            .iter_mut()
            .find(|transition| transition.id == transition_id)
            .ok_or_else(|| format!("Transition {transition_id} not found"))?;

        if let Some(mask_id) = custom_mask_id {
            if !self
                .project
                .transition_masks
                .iter()
                .any(|mask| mask.id == mask_id)
            {
                return Err(format!("Transition mask {mask_id} not found"));
            }
        }

        transition.transition_type = transition_type;
        transition.custom_mask_id = if transition_type == TransitionType::CustomMask {
            custom_mask_id
        } else {
            None
        };
        Ok(())
    }

    pub fn set_transition_speed_keyframe(
        &mut self,
        transition_id: usize,
        time_ms: i64,
        value: f32,
        mode: InterpolationMode,
    ) -> Result<(), String> {
        let transition = self
            .timeline
            .transitions
            .iter_mut()
            .find(|transition| transition.id == transition_id)
            .ok_or_else(|| format!("Transition {transition_id} not found"))?;
        transition.speed.insert_keyframe(time_ms, value, mode);
        Ok(())
    }

    pub fn set_transition_softness_keyframe(
        &mut self,
        transition_id: usize,
        time_ms: i64,
        value: f32,
        mode: InterpolationMode,
    ) -> Result<(), String> {
        let transition = self
            .timeline
            .transitions
            .iter_mut()
            .find(|transition| transition.id == transition_id)
            .ok_or_else(|| format!("Transition {transition_id} not found"))?;
        transition.softness.insert_keyframe(time_ms, value, mode);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::{MotionTrack, TrackPoint};
    use proptest::prelude::*;
    fn make_clip(id: usize, track_id: usize, start: i64, end: i64) -> Clip {
        Clip::new(id, id, track_id, start, end, 0, end - start)
    }
    #[test]
    fn scripted_session_round_trip_preserves_transition_state() {
        let mut session = EditorSession::with_default_project("Harness", 1920, 1080, 60.0, 3);
        let asset_a = session.add_asset("A", "file:///a.mp4", 2_000);
        let asset_b = session.add_asset("B", "file:///b.mp4", 2_000);
        let clip_a = session.build_clip(asset_a, 3, 0, 1_200);
        let clip_b = session.build_clip(asset_b, 3, 800, 1_600);
        let clip_a_id = session.add_clip(clip_a);
        let clip_b_id = session.add_clip(clip_b);

        assert_eq!(session.timeline.transitions.len(), 1);
        let transition_id = session.timeline.transitions[0].id;
        let mask_id = session
            .import_transition_mask("Diag", 2, 2, vec![0, 64, 192, 255])
            .expect("mask import should succeed");
        session
            .set_transition_type(transition_id, TransitionType::CustomMask, Some(mask_id))
            .expect("transition type should update");
        session
            .set_transition_speed_keyframe(transition_id, 1_000, 1.8, InterpolationMode::Bezier)
            .expect("speed keyframe should update");
        session
            .set_transition_softness_keyframe(transition_id, 1_000, 0.25, InterpolationMode::Linear)
            .expect("softness keyframe should update");
        session
            .apply_fade_in(clip_a_id)
            .expect("fade in should apply");
        session
            .apply_fade_out(clip_b_id)
            .expect("fade out should apply");
        assert!(matches!(
            session.toggle_marker(900),
            MarkerToggleResult::Added(_)
        ));
        session.sync_project_from_timeline();

        let path = std::env::temp_dir().join("clipstrike-editor-session.cstrike");
        session
            .save_to_path(&path)
            .expect("session save should succeed");

        let reopened = EditorSession::open_from_path(&path).expect("session open should succeed");
        assert_eq!(reopened.project.transition_masks.len(), 1);
        assert_eq!(reopened.timeline.transitions.len(), 1);
        assert_eq!(
            reopened.timeline.transitions[0].transition_type,
            TransitionType::CustomMask
        );
        assert_eq!(
            reopened.timeline.transitions[0].custom_mask_id,
            Some(mask_id)
        );
        assert!(
            reopened.timeline.transitions[0]
                .speed
                .keyframe(1_000)
                .is_some()
        );
        assert!(
            reopened.timeline.transitions[0]
                .softness
                .keyframe(1_000)
                .is_some()
        );
        assert_eq!(reopened.project.markers.len(), 1);
        assert_eq!(reopened.timeline.clips.len(), 2);
        assert!(
            reopened
                .timeline
                .clips
                .iter()
                .find(|clip| clip.id == clip_a_id)
                .and_then(|clip| clip.opacity.keyframe(0))
                .is_some()
        );
        assert!(
            reopened
                .timeline
                .clips
                .iter()
                .find(|clip| clip.id == clip_b_id)
                .and_then(|clip| clip.opacity.keyframe(1_600))
                .is_some()
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn session_save_reopen_smoke_test_preserves_project_path_and_timeline_state() {
        let mut session = EditorSession::with_default_project("Smoke", 1280, 720, 30.0, 2);
        let asset_id = session.add_asset("Clip", "file:///clip.mp4", 1_500);
        let clip_id = session.add_clip(session.build_clip(asset_id, 2, 50, 550));

        session.toggle_marker(400);
        session
            .apply_fade_in(clip_id)
            .expect("fade in should apply");
        session.sync_project_from_timeline();

        let path = std::env::temp_dir().join("clipstrike-session-smoke.cstrike");
        session
            .save_to_path(&path)
            .expect("session save should succeed");

        assert_eq!(
            session.current_project_path.as_deref(),
            Some(path.as_path())
        );

        let reopened = EditorSession::open_from_path(&path).expect("session open should succeed");
        assert_eq!(
            reopened.current_project_path.as_deref(),
            Some(path.as_path())
        );
        assert_eq!(reopened.project.name, "Smoke");
        assert_eq!(reopened.project.tracks.len(), 2);
        assert_eq!(reopened.project.markers.len(), 1);
        assert_eq!(reopened.timeline.clips.len(), 1);

        let reopened_clip = reopened
            .timeline
            .clips
            .iter()
            .next()
            .expect("clip should exist");
        assert_eq!((reopened_clip.start, reopened_clip.end), (50, 550));
        assert!(reopened_clip.opacity.keyframe(50).is_some());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn track_mutation_round_trip_rebuilds_timeline() {
        let mut session = EditorSession::with_default_project("Tracks", 1920, 1080, 30.0, 2);
        let asset_id = session.add_asset("Clip", "file:///clip.mp4", 2_000);
        let clip_id = session.add_clip(session.build_clip(asset_id, 2, 0, 1_000));

        session
            .apply_track_mutation(|project| {
                project.rename_track(2, "Primary")?;
                project.reorder_tracks(0, 1)
            })
            .expect("track mutation should succeed");

        assert_eq!(session.project.tracks[1].name, "Primary");
        assert!(
            session
                .timeline
                .clips
                .iter()
                .any(|clip| clip.id == clip_id && clip.track_id == 1)
        );
    }

    #[test]
    fn command_stack_round_trips_clip_edits() {
        let mut session = EditorSession::with_default_project("Commands", 1920, 1080, 60.0, 2);
        let asset_id = session.add_asset("Clip", "file:///clip.mp4", 2_000);
        let clip_id = session.add_clip(session.build_clip(asset_id, 2, 100, 500));

        session
            .move_clip(clip_id, 200, 600)
            .expect("move should succeed");
        session
            .trim_clip(clip_id, 250, 550)
            .expect("trim should succeed");
        let edited = session
            .timeline
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .expect("clip should exist after edits");
        assert_eq!((edited.start, edited.end), (250, 550));

        session.undo();
        let moved = session
            .timeline
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .expect("clip should exist after undo trim");
        assert_eq!((moved.start, moved.end), (200, 600));

        session.undo();
        let original = session
            .timeline
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .expect("clip should exist after undo move");
        assert_eq!((original.start, original.end), (100, 500));

        session.redo();
        session.redo();
        let redone = session
            .timeline
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .expect("clip should exist after redo");
        assert_eq!((redone.start, redone.end), (250, 550));

        session.remove_clip(clip_id).expect("remove should succeed");
        assert!(session.timeline.clips.iter().all(|clip| clip.id != clip_id));
        session.undo();
        assert!(session.timeline.clips.iter().any(|clip| clip.id == clip_id));
    }
    #[test]
    fn speed_multiplier_maps_timeline_time_to_source_time() {
        let mut session = EditorSession::with_default_project("Retime", 1920, 1080, 60.0, 1);
        let asset_id = session.add_asset("Clip", "file:///clip.mp4", 5_000);
        let mut clip = session.build_clip(asset_id, 1, 0, 1_000);
        clip.out_point = 2_000;
        let clip_id = session.add_clip(clip);
        session
            .set_clip_speed_keyframe(clip_id, 0, 2.0, InterpolationMode::Linear)
            .expect("speed keyframe should apply");

        assert_eq!(session.clip_source_time_at(clip_id, 0).unwrap(), 0);
        assert_eq!(session.clip_source_time_at(clip_id, 250).unwrap(), 500);
        assert_eq!(session.clip_source_time_at(clip_id, 1_000).unwrap(), 2_000);
    }

    #[test]
    fn reverse_playback_counts_back_from_total_source_span() {
        let mut session = EditorSession::with_default_project("Reverse", 1920, 1080, 60.0, 1);
        let asset_id = session.add_asset("Clip", "file:///clip.mp4", 5_000);
        let mut clip = session.build_clip(asset_id, 1, 0, 1_000);
        clip.out_point = 2_000;
        let clip_id = session.add_clip(clip);
        session
            .set_clip_speed_keyframe(clip_id, 0, 2.0, InterpolationMode::Linear)
            .expect("speed keyframe should apply");
        session
            .set_clip_reverse(clip_id, true)
            .expect("reverse should apply");

        assert_eq!(session.clip_source_time_at(clip_id, 0).unwrap(), 2_000);
        assert_eq!(session.clip_source_time_at(clip_id, 250).unwrap(), 1_500);
        assert_eq!(session.clip_source_time_at(clip_id, 1_000).unwrap(), 0);
    }

    #[test]
    fn speed_curve_integrates_variable_rate() {
        let mut clip = make_clip(1, 1, 0, 1_000);
        clip.speed
            .insert_keyframe(0, 0.5, InterpolationMode::Linear);
        clip.speed
            .insert_keyframe(1_000, 1.5, InterpolationMode::Linear);

        let halfway = clip.source_time_at(500);
        let end = clip.source_time_at(1_000);
        assert!((halfway - 375).abs() <= 10);
        assert!((end - 1_000).abs() <= 10);
    }

    #[test]
    fn freeze_frame_holds_requested_source_time() {
        let mut session = EditorSession::with_default_project("Freeze", 1920, 1080, 60.0, 1);
        let asset_id = session.add_asset("Clip", "file:///clip.mp4", 5_000);
        let clip_id = session.add_clip(session.build_clip(asset_id, 1, 100, 1_100));
        session
            .freeze_clip_at_source_time(clip_id, Some(333))
            .expect("freeze should apply");

        assert_eq!(session.clip_source_time_at(clip_id, 100).unwrap(), 333);
        assert_eq!(session.clip_source_time_at(clip_id, 600).unwrap(), 333);
        assert_eq!(session.clip_source_time_at(clip_id, 1_100).unwrap(), 333);

        session
            .freeze_clip_at_source_time(clip_id, None)
            .expect("freeze should clear");
        assert_eq!(session.clip_source_time_at(clip_id, 600).unwrap(), 500);
    }

    #[test]
    fn apply_motion_track_generates_position_keyframes() {
        let mut session = EditorSession::with_default_project("Track", 1920, 1080, 60.0, 1);
        let asset_id = session.add_asset("Clip", "file:///clip.mp4", 5_000);
        let clip_id = session.add_clip(session.build_clip(asset_id, 1, 0, 1_000));
        let track = MotionTrack {
            id: 7,
            name: "Object".to_string(),
            points: vec![
                TrackPoint {
                    time_ms: 0,
                    x: 0.1,
                    y: 0.2,
                    width: 0.3,
                    height: 0.4,
                },
                TrackPoint {
                    time_ms: 500,
                    x: 0.4,
                    y: 0.5,
                    width: 0.3,
                    height: 0.4,
                },
                TrackPoint {
                    time_ms: 1_000,
                    x: 0.7,
                    y: 0.8,
                    width: 0.3,
                    height: 0.4,
                },
            ],
        };

        session
            .apply_motion_track(clip_id, track)
            .expect("tracking should apply");

        let clip = session
            .timeline
            .clips
            .iter()
            .find(|clip| clip.id == clip_id)
            .expect("clip should exist");

        assert!(clip.motion_tracking_enabled);
        assert_eq!(clip.tracking_data.len(), 1);
        assert_eq!(clip.tracking_data[0].points.len(), 3);
        assert_eq!(clip.pos_x.keyframes.len(), 3);
        assert_eq!(clip.pos_y.keyframes.len(), 3);
        assert!((clip.pos_x.keyframes[0].value - 0.1).abs() < f32::EPSILON);
        assert!((clip.pos_y.keyframes[2].value - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn test_editor_add_asset() {
        let mut session = EditorSession::with_default_project("AssetTest", 1920, 1080, 60.0, 1);
        let id1 = session.add_asset("Video 1", "file:///vid1.mp4", 1000);
        let id2 = session.add_asset("Video 2", "file:///vid2.mp4", 2000);

        assert_eq!(session.project.assets.len(), 2);
        assert_eq!(session.project.assets[0].id, id1);
        assert_eq!(session.project.assets[1].id, id2);
    }

    #[test]
    fn test_editor_apply_fade_in_out() {
        let mut session = EditorSession::with_default_project("FadeTest", 1920, 1080, 60.0, 1);
        let asset_id = session.add_asset("Clip", "file:///clip.mp4", 5_000);
        let clip_id = session.add_clip(session.build_clip(asset_id, 1, 100, 5100));

        session
            .apply_fade_in(clip_id)
            .expect("fade in should apply");
        session
            .apply_fade_out(clip_id)
            .expect("fade out should apply");

        let clip = session
            .timeline
            .clips
            .iter()
            .find(|c| c.id == clip_id)
            .unwrap();

        assert!(clip.opacity.keyframes.len() >= 4);

        let first_kf = clip
            .opacity
            .keyframes
            .iter()
            .find(|kf| kf.time_ms == 100)
            .unwrap();
        assert_eq!(first_kf.value, 0.0);

        let last_kf = clip
            .opacity
            .keyframes
            .iter()
            .find(|kf| kf.time_ms == 5100)
            .unwrap();
        assert_eq!(last_kf.value, 0.0);
    }

    #[test]
    fn test_editor_clip_commands() {
        let mut session = EditorSession::with_default_project("CmdTest", 1920, 1080, 60.0, 1);
        let asset_id = session.add_asset("Clip", "file:///clip.mp4", 5_000);

        // Add
        let clip_id = session.add_clip(session.build_clip(asset_id, 1, 0, 1000));
        assert_eq!(session.timeline.clips.len(), 1);

        // Move
        session.move_clip(clip_id, 100, 1100).unwrap();
        assert_eq!(session.timeline.clips[0].start, 100);

        // Trim
        session.trim_clip(clip_id, 200, 1000).unwrap();
        assert_eq!(session.timeline.clips[0].start, 200);

        // Remove
        session.remove_clip(clip_id).unwrap();
        assert_eq!(session.timeline.clips.len(), 0);

        // Undo Remove
        session.undo();
        assert_eq!(session.timeline.clips.len(), 1);
        assert_eq!(session.timeline.clips[0].start, 200);

        // Undo Trim
        session.undo();
        assert_eq!(session.timeline.clips[0].start, 100);

        // Undo Move
        session.undo();
        assert_eq!(session.timeline.clips[0].start, 0);

        // Undo Add
        session.undo();
        assert_eq!(session.timeline.clips.len(), 0);
    }

    proptest! {
        #[test]
        fn auto_transition_range_matches_overlap(
            start_a in 0i64..5_000,
            len_a in 1i64..2_000,
            start_b in 0i64..5_000,
            len_b in 1i64..2_000,
        ) {
            let mut timeline = Timeline::new();
            timeline.add_clip(make_clip(1, 1, start_a, start_a + len_a));
            timeline.add_clip(make_clip(2, 1, start_b, start_b + len_b));

            let overlap_start = start_a.max(start_b);
            let overlap_end = (start_a + len_a).min(start_b + len_b);
            if overlap_start < overlap_end {
                prop_assert_eq!(timeline.transitions.len(), 1);
                prop_assert_eq!(timeline.transitions[0].start_ms, overlap_start);
                prop_assert_eq!(timeline.transitions[0].end_ms, overlap_end);
            } else {
                prop_assert!(timeline.transitions.is_empty());
            }
        }

        #[test]
        fn reordered_project_keeps_clip_and_transition_track_ids_valid(
            track_count in 2usize..6,
            from_index in 0usize..5,
            to_index in 0usize..5,
            clip_track_seed in proptest::collection::vec(0usize..5, 1..8),
            transition_track_seed in proptest::collection::vec(0usize..5, 0..6),
        ) {
            let from_index = from_index % track_count;
            let to_index = to_index % track_count;

            let mut project = Project::new("Prop", 1920, 1080, 60.0);
            project.tracks = Project::default_tracks(track_count);
            let track_ids: Vec<usize> = project.tracks.iter().map(|track| track.id).collect();

            for (idx, seed) in clip_track_seed.iter().enumerate() {
                let track_id = track_ids[*seed % track_ids.len()];
                project.clips.push(make_clip(idx + 1, track_id, (idx as i64) * 100, (idx as i64) * 100 + 50));
            }

            for (idx, seed) in transition_track_seed.iter().enumerate() {
                let track_id = track_ids[*seed % track_ids.len()];
                project.transitions.push(crate::transitions::Transition::new(
                    idx + 100,
                    track_id,
                    (idx as i64) * 20,
                    (idx as i64) * 20 + 10,
                    TransitionType::Crossfade,
                ));
            }

            let clip_ids_before: Vec<usize> = project.clips.iter().map(|clip| clip.id).collect();
            let transition_ids_before: Vec<usize> = project.transitions.iter().map(|transition| transition.id).collect();

            project.reorder_tracks(from_index, to_index).expect("reorder should succeed");

            let valid_track_ids: Vec<usize> = project.tracks.iter().map(|track| track.id).collect();
            prop_assert_eq!(project.clips.iter().map(|clip| clip.id).collect::<Vec<_>>(), clip_ids_before);
            prop_assert_eq!(project.transitions.iter().map(|transition| transition.id).collect::<Vec<_>>(), transition_ids_before);
            prop_assert!(project.clips.iter().all(|clip| valid_track_ids.contains(&clip.track_id)));
            prop_assert!(project.transitions.iter().all(|transition| valid_track_ids.contains(&transition.track_id)));
        }
    }
}
