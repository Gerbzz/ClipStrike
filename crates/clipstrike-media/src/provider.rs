use std::time::{Duration, Instant};

/// A simple frame provider that syncs the engine's playback with wgpu's refresh rate.
/// In a full implementation, this uses a high-precision monotonic clock and
/// checks the GST clock against the wgpu vsync clock.
pub struct FrameProvider {
    start_time: Option<Instant>,
    is_playing: bool,
}

impl FrameProvider {
    pub fn new() -> Self {
        Self {
            start_time: None,
            is_playing: false,
        }
    }

    pub fn play(&mut self) {
        if !self.is_playing {
            self.start_time = Some(Instant::now());
            self.is_playing = true;
        }
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
    }

    /// Calculates what timestamp the engine should be at based on the wall clock.
    pub fn current_playback_time(&self) -> Duration {
        if self.is_playing {
            if let Some(start) = self.start_time {
                return start.elapsed();
            }
        }
        Duration::ZERO
    }
}
