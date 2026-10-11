use crate::project::Project;
use crate::timeline::{Clip, Timeline};

pub trait Command {
    fn execute(&mut self, project: &mut Project, timeline: &mut Timeline);
    fn undo(&mut self, project: &mut Project, timeline: &mut Timeline);
}

pub struct CompoundCommand {
    pub commands: Vec<Box<dyn Command>>,
}

impl Command for CompoundCommand {
    fn execute(&mut self, project: &mut Project, timeline: &mut Timeline) {
        for cmd in &mut self.commands {
            cmd.execute(project, timeline);
        }
    }
    fn undo(&mut self, project: &mut Project, timeline: &mut Timeline) {
        for cmd in self.commands.iter_mut().rev() {
            cmd.undo(project, timeline);
        }
    }
}

pub struct AddClipCommand {
    pub clip: Clip,
}

impl Command for AddClipCommand {
    fn execute(&mut self, _project: &mut Project, timeline: &mut Timeline) {
        timeline.add_clip(self.clip.clone());
    }
    fn undo(&mut self, _project: &mut Project, timeline: &mut Timeline) {
        timeline.remove_clip(self.clip.id);
    }
}

pub struct RemoveClipCommand {
    pub clip: Clip,
}

impl Command for RemoveClipCommand {
    fn execute(&mut self, _project: &mut Project, timeline: &mut Timeline) {
        timeline.remove_clip(self.clip.id);
    }
    fn undo(&mut self, _project: &mut Project, timeline: &mut Timeline) {
        timeline.add_clip(self.clip.clone());
    }
}

pub struct MoveClipCommand {
    pub clip_id: usize,
    pub old_start: i64,
    pub old_end: i64,
    pub new_start: i64,
    pub new_end: i64,
    pub old_track_id: usize,
    pub new_track_id: usize,
}

impl Command for MoveClipCommand {
    fn execute(&mut self, _project: &mut Project, timeline: &mut Timeline) {
        let _ = timeline.move_clip(self.clip_id, self.new_start, self.new_end);
        let _ = timeline.move_clip_to_track(self.clip_id, self.new_track_id);
    }
    fn undo(&mut self, _project: &mut Project, timeline: &mut Timeline) {
        let _ = timeline.move_clip(self.clip_id, self.old_start, self.old_end);
        let _ = timeline.move_clip_to_track(self.clip_id, self.old_track_id);
    }
}

pub struct TrimClipCommand {
    pub clip_id: usize,
    pub old_start: i64,
    pub old_end: i64,
    pub new_start: i64,
    pub new_end: i64,
    pub original_clip: Option<Clip>,
    pub trimmed_clip: Option<Clip>,
}

impl Command for TrimClipCommand {
    fn execute(&mut self, project: &mut Project, timeline: &mut Timeline) {
        if let Some(trimmed) = &self.trimmed_clip {
            let _ = timeline.replace_clip(trimmed.clone());
            return;
        }
        let Some(original) = timeline.clips.iter().find(|c| c.id == self.clip_id).cloned() else {
            return;
        };
        let max_duration = project
            .assets
            .iter()
            .find(|a| {
                timeline
                    .clips
                    .iter()
                    .find(|c| c.id == self.clip_id)
                    .map(|c| c.asset_id == a.id)
                    .unwrap_or(false)
            })
            .map(|a| a.duration_ms as i64)
            .unwrap_or(i64::MAX);

        if timeline.trim(
            self.clip_id,
            self.new_start,
            self.new_end,
            max_duration,
        ).is_ok() {
            self.original_clip = Some(original);
            self.trimmed_clip = timeline.clips.iter().find(|c| c.id == self.clip_id).cloned();
        }
    }
    fn undo(&mut self, _project: &mut Project, timeline: &mut Timeline) {
        if let Some(original) = &self.original_clip {
            let _ = timeline.replace_clip(original.clone());
        }
    }
}

pub struct SeparateAudioCommand {
    pub clip_id: usize,
    pub audio_clip: Clip,
    pub original_clip: Option<Clip>,
}

impl Command for SeparateAudioCommand {
    fn execute(&mut self, project: &mut Project, timeline: &mut Timeline) {
        if let Some(pos) = timeline.clips.iter().position(|c| c.id == self.clip_id) {
            let original = timeline.clips[pos].clone();
            let mut video_only = original.clone();
            video_only.audio_enabled = false;
            video_only.group_id = self.audio_clip.group_id;
            self.original_clip = Some(original);
            let _ = timeline.replace_clip(video_only);
            timeline.add_clip(self.audio_clip.clone());

            // Sync project
            if let Some(p_pos) = project.clips.iter().position(|c| c.id == self.clip_id) {
                project.clips[p_pos].audio_enabled = false;
                project.clips[p_pos].group_id = self.audio_clip.group_id;
            }
            project.clips.push(self.audio_clip.clone());
        }
    }
    fn undo(&mut self, project: &mut Project, timeline: &mut Timeline) {
        let Some(original) = &self.original_clip else { return; };
        timeline.remove_clip(self.audio_clip.id);
        let _ = timeline.replace_clip(original.clone());

        // Sync project
        project.clips.retain(|c| c.id != self.audio_clip.id);
        if let Some(p_pos) = project.clips.iter().position(|c| c.id == self.clip_id) {
            project.clips[p_pos].audio_enabled = original.audio_enabled;
            project.clips[p_pos].group_id = original.group_id;
        }
    }
}

pub struct AddTrackCommand {
    pub track_id: Option<usize>,
}

impl Command for AddTrackCommand {
    fn execute(&mut self, project: &mut Project, _timeline: &mut Timeline) {
        let id = project.add_track();
        self.track_id = Some(id);
    }
    fn undo(&mut self, project: &mut Project, _timeline: &mut Timeline) {
        if let Some(id) = self.track_id {
            if project.tracks.len() == 1 && project.tracks[0].id == id {
                project.tracks.clear();
            } else {
                let _ = project.remove_track(id);
            }
        }
    }
}

pub struct RazorClipCommand {
    pub clip_id: usize,
    pub split_at: i64,
    pub original_clip: Option<Clip>,
    pub new_clip_id: Option<usize>,
    pub split_clips: Option<(Clip, Clip)>,
}

impl Command for RazorClipCommand {
    fn execute(&mut self, _project: &mut Project, timeline: &mut Timeline) {
        if let Some((left, right)) = &self.split_clips {
            let original_index = timeline.clips.iter().position(|c| c.id == self.clip_id);
            timeline.remove_clip(self.clip_id);
            timeline.add_clip(left.clone());
            timeline.add_clip(right.clone());
            if let Some(index) = original_index {
                let _ = timeline.move_clip_to_index(left.id, index);
                let _ = timeline.move_clip_to_index(right.id, index + 1);
            }
            return;
        }
        if let Some(pos) = timeline.clips.iter().position(|c| c.id == self.clip_id) {
            let clip = timeline.clips[pos].clone();
            if timeline.razor(self.clip_id, self.split_at).is_ok() {
                self.original_clip = Some(clip);
                if let (Some(left), Some(right)) = (
                    timeline.clips.get(pos).cloned(),
                    timeline.clips.get(pos + 1).cloned(),
                ) {
                    self.new_clip_id = Some(right.id);
                    self.split_clips = Some((left, right));
                }
            }
        }
    }
    fn undo(&mut self, _project: &mut Project, timeline: &mut Timeline) {
        if let (Some(original), Some(new_id)) = (&self.original_clip, self.new_clip_id) {
            let original_index = timeline.clips.iter().position(|c| c.id == self.clip_id);
            // Remove both segments
            timeline.remove_clip(self.clip_id);
            timeline.remove_clip(new_id);
            // Restore original
            timeline.add_clip(original.clone());
            if let Some(index) = original_index {
                let _ = timeline.move_clip_to_index(original.id, index);
            }
        }
    }
}

pub struct UndoStack {
    undo: Vec<Box<dyn Command>>,
    redo: Vec<Box<dyn Command>>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn push(
        &mut self,
        mut command: Box<dyn Command>,
        project: &mut Project,
        timeline: &mut Timeline,
    ) {
        command.execute(project, timeline);
        self.undo.push(command);
        self.redo.clear();
    }

    pub fn undo(&mut self, project: &mut Project, timeline: &mut Timeline) {
        if let Some(mut command) = self.undo.pop() {
            command.undo(project, timeline);
            self.redo.push(command);
        }
    }

    pub fn redo(&mut self, project: &mut Project, timeline: &mut Timeline) {
        if let Some(mut command) = self.redo.pop() {
            command.execute(project, timeline);
            self.undo.push(command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframes::KeyframedProperty;

    fn create_test_clip(id: usize, track_id: usize, start: i64, end: i64) -> Clip {
        Clip {
            id,
            asset_id: 99,
            track_id,
            start,
            end,
            in_point: 0,
            out_point: end - start,
            blend_mode: crate::timeline::BlendMode::Normal,
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
            stabilization_strength: KeyframedProperty::new(0.5),
            object_detection_enabled: false,
            group_id: None,
            color_tag: [80, 120, 160, 255],
        }
    }

    #[test]
    fn test_add_clip_command() {
        let mut timeline = Timeline::new();
        let mut project = Project::new("test", 1920, 1080, 30.0);
        let mut undo_stack = UndoStack::new();
        let clip = create_test_clip(1, 0, 0, 1000);

        // Execute Add
        undo_stack.push(
            Box::new(AddClipCommand { clip: clip.clone() }),
            &mut project,
            &mut timeline,
        );
        assert_eq!(timeline.clips.len(), 1);
        assert_eq!(timeline.clips[0].id, 1);

        // Undo Add
        undo_stack.undo(&mut project, &mut timeline);
        assert_eq!(timeline.clips.len(), 0);

        // Redo Add
        undo_stack.redo(&mut project, &mut timeline);
        assert_eq!(timeline.clips.len(), 1);
        assert_eq!(timeline.clips[0].id, 1);
    }

    #[test]
    fn test_trim_clip_command() {
        let mut timeline = Timeline::new();
        let mut project = Project::new("test", 1920, 1080, 30.0);
        let mut undo_stack = UndoStack::new();
        let mut clip = create_test_clip(1, 0, 100, 2000);
        clip.in_point = 250;
        clip.out_point = 2150;
        timeline.add_clip(clip);

        // Trim clip (change start and end)
        undo_stack.push(
            Box::new(TrimClipCommand {
                clip_id: 1,
                old_start: 100,
                old_end: 2000,
                new_start: 500,
                new_end: 1500,
                original_clip: None,
                trimmed_clip: None,
            }),
            &mut project,
            &mut timeline,
        );

        assert_eq!(timeline.clips[0].start, 500);
        assert_eq!(timeline.clips[0].end, 1500);
        assert_eq!((timeline.clips[0].in_point, timeline.clips[0].out_point), (650, 1650));

        // Undo trim
        undo_stack.undo(&mut project, &mut timeline);
        assert_eq!(timeline.clips[0].start, 100);
        assert_eq!(timeline.clips[0].end, 2000);
        assert_eq!((timeline.clips[0].in_point, timeline.clips[0].out_point), (250, 2150));

        // Redo trim
        undo_stack.redo(&mut project, &mut timeline);
        assert_eq!(timeline.clips[0].start, 500);
        assert_eq!(timeline.clips[0].end, 1500);
        assert_eq!((timeline.clips[0].in_point, timeline.clips[0].out_point), (650, 1650));
    }

    #[test]
    fn invalid_razor_undo_preserves_unrelated_clip() {
        let mut timeline = Timeline::new();
        let mut project = Project::new("test", 1920, 1080, 30.0);
        let original = create_test_clip(1, 0, 0, 1000);
        let unrelated = create_test_clip(2, 0, 2000, 3000);
        timeline.add_clip(original.clone());
        timeline.add_clip(unrelated.clone());
        let mut undo = UndoStack::new();
        undo.push(Box::new(RazorClipCommand {
            clip_id: 1, split_at: 1000, original_clip: None,
            new_clip_id: None, split_clips: None,
        }), &mut project, &mut timeline);
        undo.undo(&mut project, &mut timeline);
        assert_eq!(timeline.clips, vec![original, unrelated]);
    }

    #[test]
    fn razor_redo_keeps_clip_identity_and_blend() {
        let mut timeline = Timeline::new();
        let mut project = Project::new("test", 1920, 1080, 30.0);
        let before = create_test_clip(9, 0, 0, 100);
        let after = create_test_clip(10, 0, 1100, 1200);
        timeline.add_clip(before.clone());
        let mut original = create_test_clip(1, 0, 100, 1100);
        original.in_point = 200;
        original.out_point = 1200;
        original.blend_mode = crate::timeline::BlendMode::Multiply;
        original.reverse = true;
        timeline.add_clip(original.clone());
        timeline.add_clip(after.clone());
        let mut undo = UndoStack::new();
        undo.push(Box::new(RazorClipCommand {
            clip_id: 1, split_at: 600, original_clip: None,
            new_clip_id: None, split_clips: None,
        }), &mut project, &mut timeline);
        let split = timeline.clips.clone();
        assert_eq!(split.len(), 4);
        assert_eq!(split[1].blend_mode, crate::timeline::BlendMode::Multiply);
        assert_eq!(split[2].blend_mode, crate::timeline::BlendMode::Multiply);
        assert_eq!((split[1].in_point, split[1].out_point), (700, 1200));
        assert_eq!((split[2].in_point, split[2].out_point), (200, 700));
        undo.undo(&mut project, &mut timeline);
        assert_eq!(timeline.clips, vec![before, original, after]);
        undo.redo(&mut project, &mut timeline);
        assert_eq!(timeline.clips, split);
    }

    #[test]
    fn test_separate_audio_command() {
        let mut timeline = Timeline::new();
        let mut project = Project::new("test", 1920, 1080, 30.0);
        let mut undo_stack = UndoStack::new();

        let mut original_clip = create_test_clip(1, 0, 0, 1000);
        original_clip.audio_enabled = true;
        original_clip.group_id = Some(42);
        timeline.add_clip(original_clip.clone());
        project.clips.push(original_clip);

        let mut audio_clip = create_test_clip(2, 1, 0, 1000);
        audio_clip.video_enabled = false;
        audio_clip.audio_enabled = true;
        audio_clip.group_id = Some(99);

        undo_stack.push(
            Box::new(SeparateAudioCommand {
                clip_id: 1,
                audio_clip,
                original_clip: None,
            }),
            &mut project,
            &mut timeline,
        );

        // Post-execution: original clip is now video-only, new clip is audio-only
        assert_eq!(timeline.clips.len(), 2);

        let vid_clip = timeline.clips.iter().find(|c| c.id == 1).unwrap();
        assert!(!vid_clip.audio_enabled);
        assert!(!timeline.get_clips_at_time(500).iter().find(|c| c.id == 1).unwrap().audio_enabled);

        let aud_clip = timeline.clips.iter().find(|c| c.id == 2).unwrap();
        assert!(!aud_clip.video_enabled);
        assert!(aud_clip.audio_enabled);

        // Undo: should remove the audio clip and re-enable audio on video clip
        undo_stack.undo(&mut project, &mut timeline);
        assert_eq!(timeline.clips.len(), 1);

        let vid_clip_undone = timeline.clips.iter().find(|c| c.id == 1).unwrap();
        assert!(vid_clip_undone.audio_enabled);
        assert_eq!(vid_clip_undone.group_id, Some(42));
        assert_eq!(project.clips.len(), 1);
        assert_eq!(project.clips[0].group_id, Some(42));
    }

    #[test]
    fn test_remove_clip_command() {
        let mut timeline = Timeline::new();
        let mut project = Project::new("test", 1920, 1080, 30.0);
        let mut undo_stack = UndoStack::new();
        let clip = create_test_clip(1, 0, 0, 1000);

        // Add it directly first
        timeline.add_clip(clip.clone());
        assert_eq!(timeline.clips.len(), 1);

        // Execute Remove
        undo_stack.push(
            Box::new(RemoveClipCommand { clip }),
            &mut project,
            &mut timeline,
        );
        assert_eq!(timeline.clips.len(), 0);

        // Undo Remove (Should add it back)
        undo_stack.undo(&mut project, &mut timeline);
        assert_eq!(timeline.clips.len(), 1);
        assert_eq!(timeline.clips[0].id, 1);

        // Redo Remove (Should remove it again)
        undo_stack.redo(&mut project, &mut timeline);
        assert_eq!(timeline.clips.len(), 0);
    }

    #[test]
    fn test_move_clip_command_updates_track_id() {
        let mut timeline = Timeline::new();
        let mut project = Project::new("test", 1920, 1080, 30.0);
        let mut undo_stack = UndoStack::new();
        let clip = create_test_clip(1, 1, 100, 1100);

        timeline.add_clip(clip);

        undo_stack.push(
            Box::new(MoveClipCommand {
                clip_id: 1,
                old_start: 100,
                old_end: 1100,
                new_start: 100,
                new_end: 1100,
                old_track_id: 1,
                new_track_id: 2,
            }),
            &mut project,
            &mut timeline,
        );

        assert_eq!(timeline.clips[0].track_id, 2);

        undo_stack.undo(&mut project, &mut timeline);
        assert_eq!(timeline.clips[0].track_id, 1);

        undo_stack.redo(&mut project, &mut timeline);
        assert_eq!(timeline.clips[0].track_id, 2);
    }
    #[test]
    fn test_move_clip_command() {
        let mut timeline = Timeline::new();
        let mut project = Project::new("test", 1920, 1080, 30.0);
        let mut undo_stack = UndoStack::new();
        let clip = create_test_clip(1, 0, 100, 1100);

        timeline.add_clip(clip);

        // Execute Move (shift right by 400ms)
        undo_stack.push(
            Box::new(MoveClipCommand {
                clip_id: 1,
                old_start: 100,
                old_end: 1100,
                new_start: 500,
                new_end: 1500,
                old_track_id: 0,
                new_track_id: 0,
            }),
            &mut project,
            &mut timeline,
        );

        assert_eq!(timeline.clips[0].start, 500);
        assert_eq!(timeline.clips[0].end, 1500);

        // Undo Move
        undo_stack.undo(&mut project, &mut timeline);
        assert_eq!(timeline.clips[0].start, 100);
        assert_eq!(timeline.clips[0].end, 1100);

        // Redo Move
        undo_stack.redo(&mut project, &mut timeline);
        assert_eq!(timeline.clips[0].start, 500);
        assert_eq!(timeline.clips[0].end, 1500);
    }

    #[test]
    fn test_undo_stack_push_clears_redo() {
        let mut timeline = Timeline::new();
        let mut project = Project::new("test", 1920, 1080, 30.0);
        let mut undo_stack = UndoStack::new();

        // Push 1, Push 2
        let clip1 = create_test_clip(1, 0, 0, 100);
        undo_stack.push(
            Box::new(AddClipCommand { clip: clip1 }),
            &mut project,
            &mut timeline,
        );

        let clip2 = create_test_clip(2, 0, 100, 200);
        undo_stack.push(
            Box::new(AddClipCommand { clip: clip2 }),
            &mut project,
            &mut timeline,
        );

        assert_eq!(timeline.clips.len(), 2);

        // Undo to pop into Redo stack
        undo_stack.undo(&mut project, &mut timeline);
        assert_eq!(timeline.clips.len(), 1);

        // Push 3 - This should clear the Redo stack
        let clip3 = create_test_clip(3, 0, 200, 300);
        undo_stack.push(
            Box::new(AddClipCommand { clip: clip3 }),
            &mut project,
            &mut timeline,
        );

        // Now if we redo, nothing should happen
        let state_before_redo = timeline.clips.clone();
        undo_stack.redo(&mut project, &mut timeline);
        assert_eq!(timeline.clips, state_before_redo);
        assert_eq!(timeline.clips.len(), 2);
        assert_eq!(timeline.clips[0].id, 1);
        assert_eq!(timeline.clips[1].id, 3);
    }

    #[test]
    fn test_add_track_command() {
        let mut timeline = Timeline::new();
        let mut project = Project::new("test", 1920, 1080, 30.0);
        let mut undo_stack = UndoStack::new();

        let initial_tracks = project.tracks.len();

        // Execute Add Track
        undo_stack.push(
            Box::new(AddTrackCommand { track_id: None }),
            &mut project,
            &mut timeline,
        );
        assert_eq!(project.tracks.len(), initial_tracks + 1);

        // Undo Add Track
        undo_stack.undo(&mut project, &mut timeline);
        assert_eq!(project.tracks.len(), initial_tracks);

        // Redo Add Track
        undo_stack.redo(&mut project, &mut timeline);
        assert_eq!(project.tracks.len(), initial_tracks + 1);
    }
}
