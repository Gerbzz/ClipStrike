use std::time::Duration;

pub fn export_frame_count_for_duration_ms(duration_ms: i64, fps_num: i32, fps_den: i32) -> u64 {
    let numerator = duration_ms.max(1) as u128 * fps_num.max(1) as u128;
    let denominator = 1000u128 * fps_den.max(1) as u128;
    numerator.div_ceil(denominator).min(u64::MAX as u128) as u64
}

pub fn export_frame_time_ms(frame_index: u64, fps_num: i32, fps_den: i32) -> i64 {
    let numerator = frame_index as u128 * 1000u128 * fps_den.max(1) as u128;
    (numerator / fps_num.max(1) as u128).min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod frame_timing_tests {
    use super::*;

    #[test]
    fn thirty_fps_one_hour_has_no_millisecond_drift() {
        assert_eq!(export_frame_count_for_duration_ms(3_600_000, 30, 1), 108_000);
        assert_eq!(export_frame_time_ms(107_999, 30, 1), 3_599_966);
    }

    #[test]
    fn fractional_fps_uses_rational_frame_boundaries() {
        assert_eq!(export_frame_count_for_duration_ms(10_000, 30_000, 1001), 300);
        assert_eq!(export_frame_time_ms(300, 30_000, 1001), 10_010);
    }
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
