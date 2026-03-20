use std::time::Duration;

pub fn export_frame_count_for_duration_ms(duration_ms: i64, frame_duration_ms: i64) -> u64 {
    let duration_ms = duration_ms.max(1) as u64;
    let frame_duration_ms = frame_duration_ms.max(1) as u64;
    duration_ms.saturating_add(frame_duration_ms - 1) / frame_duration_ms
}

#[derive(Debug, Clone)]
pub struct ExportProgress {
    total_frames: u64,
    completed_frames: u64,
    cancel_requested: bool,
}

impl ExportProgress {
    pub fn new(total_frames: u64) -> Self {
        Self {
            total_frames: total_frames.max(1),
            completed_frames: 0,
            cancel_requested: false,
        }
    }

    pub fn mark_frame_complete(&mut self) {
        self.completed_frames = self
            .completed_frames
            .saturating_add(1)
            .min(self.total_frames);
    }

    pub fn progress_fraction(&self) -> f32 {
        (self.completed_frames as f32 / self.total_frames as f32).clamp(0.0, 1.0)
    }

    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    pub fn completed_frames(&self) -> u64 {
        self.completed_frames
    }

    pub fn is_finished(&self) -> bool {
        self.completed_frames >= self.total_frames
    }

    pub fn estimated_seconds_remaining(&self, elapsed: Duration) -> Option<f32> {
        if self.completed_frames == 0 {
            return None;
        }

        let seconds_per_frame = elapsed.as_secs_f32() / self.completed_frames as f32;
        Some((self.total_frames.saturating_sub(self.completed_frames) as f32) * seconds_per_frame)
    }

    pub fn request_cancel(&mut self) {
        self.cancel_requested = true;
    }

    pub fn cancel_requested(&self) -> bool {
        self.cancel_requested
    }
}
