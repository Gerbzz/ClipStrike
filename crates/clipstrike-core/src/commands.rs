use crate::timeline::{Clip, Timeline};

pub trait Command {
    fn execute(&mut self, timeline: &mut Timeline);
    fn undo(&mut self, timeline: &mut Timeline);
}

pub struct AddClipCommand {
    pub clip: Clip,
}

impl Command for AddClipCommand {
    fn execute(&mut self, timeline: &mut Timeline) {
        timeline.add_clip(self.clip.clone());
    }
    fn undo(&mut self, timeline: &mut Timeline) {
        timeline.remove_clip(self.clip.id);
    }
}

pub struct RemoveClipCommand {
    pub clip: Clip,
}

impl Command for RemoveClipCommand {
    fn execute(&mut self, timeline: &mut Timeline) {
        timeline.remove_clip(self.clip.id);
    }
    fn undo(&mut self, timeline: &mut Timeline) {
        timeline.add_clip(self.clip.clone());
    }
}

pub struct MoveClipCommand {
    pub clip_id: usize,
    pub old_start: i64,
    pub old_end: i64,
    pub new_start: i64,
    pub new_end: i64,
}

impl Command for MoveClipCommand {
    fn execute(&mut self, timeline: &mut Timeline) {
        let _ = timeline.move_clip(self.clip_id, self.new_start, self.new_end);
    }
    fn undo(&mut self, timeline: &mut Timeline) {
        let _ = timeline.move_clip(self.clip_id, self.old_start, self.old_end);
    }
}

pub struct TrimClipCommand {
    pub clip_id: usize,
    pub old_start: i64,
    pub old_end: i64,
    pub new_start: i64,
    pub new_end: i64,
}

impl Command for TrimClipCommand {
    fn execute(&mut self, timeline: &mut Timeline) {
        let _ = timeline.trim(self.clip_id, self.new_start, self.new_end);
    }
    fn undo(&mut self, timeline: &mut Timeline) {
        let _ = timeline.trim(self.clip_id, self.old_start, self.old_end);
    }
}

pub struct SeparateAudioCommand {
    pub clip_id: usize,
    pub audio_clip: Clip,
}

impl Command for SeparateAudioCommand {
    fn execute(&mut self, timeline: &mut Timeline) {
        if let Some(pos) = timeline.clips.iter().position(|c| c.id == self.clip_id) {
            timeline.clips[pos].audio_enabled = false;
            timeline.add_clip(self.audio_clip.clone());
        }
    }
    fn undo(&mut self, timeline: &mut Timeline) {
        timeline.remove_clip(self.audio_clip.id);
        if let Some(pos) = timeline.clips.iter().position(|c| c.id == self.clip_id) {
            timeline.clips[pos].audio_enabled = true;
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

    pub fn push(&mut self, mut command: Box<dyn Command>, timeline: &mut Timeline) {
        command.execute(timeline);
        self.undo.push(command);
        self.redo.clear();
    }

    pub fn undo(&mut self, timeline: &mut Timeline) {
        if let Some(mut command) = self.undo.pop() {
            command.undo(timeline);
            self.redo.push(command);
        }
    }

    pub fn redo(&mut self, timeline: &mut Timeline) {
        if let Some(mut command) = self.redo.pop() {
            command.execute(timeline);
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
            reverse: false,
            freeze_frame_ms: None,
            audio_enabled: true,
            video_enabled: true,
        }
    }

    #[test]
    fn test_add_clip_command() {
        let mut timeline = Timeline::new();
        let mut undo_stack = UndoStack::new();
        let clip = create_test_clip(1, 0, 0, 1000);

        // Execute Add
        undo_stack.push(Box::new(AddClipCommand { clip: clip.clone() }), &mut timeline);
        assert_eq!(timeline.clips.len(), 1);
        assert_eq!(timeline.clips[0].id, 1);

        // Undo Add
        undo_stack.undo(&mut timeline);
        assert_eq!(timeline.clips.len(), 0);

        // Redo Add
        undo_stack.redo(&mut timeline);
        assert_eq!(timeline.clips.len(), 1);
        assert_eq!(timeline.clips[0].id, 1);
    }

    #[test]
    fn test_trim_clip_command() {
        let mut timeline = Timeline::new();
        let mut undo_stack = UndoStack::new();
        let clip = create_test_clip(1, 0, 100, 2000);
        timeline.add_clip(clip);

        // Trim clip (change start and end)
        undo_stack.push(
            Box::new(TrimClipCommand {
                clip_id: 1,
                old_start: 100,
                old_end: 2000,
                new_start: 500,
                new_end: 1500,
            }),
            &mut timeline,
        );

        assert_eq!(timeline.clips[0].start, 500);
        assert_eq!(timeline.clips[0].end, 1500);

        // Undo trim
        undo_stack.undo(&mut timeline);
        assert_eq!(timeline.clips[0].start, 100);
        assert_eq!(timeline.clips[0].end, 2000);

        // Redo trim
        undo_stack.redo(&mut timeline);
        assert_eq!(timeline.clips[0].start, 500);
        assert_eq!(timeline.clips[0].end, 1500);
    }

    #[test]
    fn test_separate_audio_command() {
        let mut timeline = Timeline::new();
        let mut undo_stack = UndoStack::new();
        
        let mut original_clip = create_test_clip(1, 0, 0, 1000);
        original_clip.audio_enabled = true;
        timeline.add_clip(original_clip);

        let mut audio_clip = create_test_clip(2, 1, 0, 1000);
        audio_clip.video_enabled = false;
        audio_clip.audio_enabled = true;

        undo_stack.push(
            Box::new(SeparateAudioCommand {
                clip_id: 1,
                audio_clip,
            }),
            &mut timeline,
        );

        // Post-execution: original clip is now video-only, new clip is audio-only
        assert_eq!(timeline.clips.len(), 2);
        
        let vid_clip = timeline.clips.iter().find(|c| c.id == 1).unwrap();
        assert!(!vid_clip.audio_enabled);
        
        let aud_clip = timeline.clips.iter().find(|c| c.id == 2).unwrap();
        assert!(!aud_clip.video_enabled);
        assert!(aud_clip.audio_enabled);

        // Undo: should remove the audio clip and re-enable audio on video clip
        undo_stack.undo(&mut timeline);
        assert_eq!(timeline.clips.len(), 1);
        
        let vid_clip_undone = timeline.clips.iter().find(|c| c.id == 1).unwrap();
        assert!(vid_clip_undone.audio_enabled);
    }

    #[test]
    fn test_remove_clip_command() {
        let mut timeline = Timeline::new();
        let mut undo_stack = UndoStack::new();
        let clip = create_test_clip(1, 0, 0, 1000);
        
        // Add it directly first
        timeline.add_clip(clip.clone());
        assert_eq!(timeline.clips.len(), 1);

        // Execute Remove
        undo_stack.push(Box::new(RemoveClipCommand { clip }), &mut timeline);
        assert_eq!(timeline.clips.len(), 0);

        // Undo Remove (Should add it back)
        undo_stack.undo(&mut timeline);
        assert_eq!(timeline.clips.len(), 1);
        assert_eq!(timeline.clips[0].id, 1);

        // Redo Remove (Should remove it again)
        undo_stack.redo(&mut timeline);
        assert_eq!(timeline.clips.len(), 0);
    }

    #[test]
    fn test_move_clip_command() {
        let mut timeline = Timeline::new();
        let mut undo_stack = UndoStack::new();
        let clip = create_test_clip(1, 0, 100, 1100);
        
        timeline.add_clip(clip);

        // Execute Move (shift right by 400ms)
        undo_stack.push(Box::new(MoveClipCommand {
            clip_id: 1,
            old_start: 100,
            old_end: 1100,
            new_start: 500,
            new_end: 1500,
        }), &mut timeline);

        assert_eq!(timeline.clips[0].start, 500);
        assert_eq!(timeline.clips[0].end, 1500);

        // Undo Move
        undo_stack.undo(&mut timeline);
        assert_eq!(timeline.clips[0].start, 100);
        assert_eq!(timeline.clips[0].end, 1100);

        // Redo Move
        undo_stack.redo(&mut timeline);
        assert_eq!(timeline.clips[0].start, 500);
        assert_eq!(timeline.clips[0].end, 1500);
    }

    #[test]
    fn test_undo_stack_push_clears_redo() {
        let mut timeline = Timeline::new();
        let mut undo_stack = UndoStack::new();
        
        // Push 1, Push 2
        let clip1 = create_test_clip(1, 0, 0, 100);
        undo_stack.push(Box::new(AddClipCommand { clip: clip1 }), &mut timeline);
        
        let clip2 = create_test_clip(2, 0, 100, 200);
        undo_stack.push(Box::new(AddClipCommand { clip: clip2 }), &mut timeline);
        
        assert_eq!(timeline.clips.len(), 2);

        // Undo to pop into Redo stack
        undo_stack.undo(&mut timeline);
        assert_eq!(timeline.clips.len(), 1);
        
        // Push 3 - This should clear the Redo stack
        let clip3 = create_test_clip(3, 0, 200, 300);
        undo_stack.push(Box::new(AddClipCommand { clip: clip3 }), &mut timeline);

        // Now if we redo, nothing should happen
        let state_before_redo = timeline.clips.clone();
        undo_stack.redo(&mut timeline);
        assert_eq!(timeline.clips, state_before_redo);
        assert_eq!(timeline.clips.len(), 2);
        assert_eq!(timeline.clips[0].id, 1);
        assert_eq!(timeline.clips[1].id, 3);
    }
}
