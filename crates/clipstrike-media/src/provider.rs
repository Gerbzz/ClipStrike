use std::time::{Duration, Instant};

/// A simple frame provider that syncs the engine's playback with wgpu's refresh rate.
/// In a full implementation, this uses a high-precision monotonic clock and
/// checks the GST clock against the wgpu vsync clock.
pub struct FrameProvider {
    start_time: Option<Instant>,
    playback_offset: Duration,
    is_playing: bool,
}

impl Default for FrameProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameProvider {
    pub fn new() -> Self {
        Self {
            start_time: None,
            playback_offset: Duration::ZERO,
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
        if self.is_playing {
            if let Some(start) = self.start_time {
                self.playback_offset += start.elapsed();
            }
            self.is_playing = false;
        }
    }

    pub fn seek(&mut self, position: Duration) {
        self.playback_offset = position;
        if self.is_playing {
            self.start_time = Some(Instant::now());
        }
    }

    /// Task 2.3: Clock synchronization logic
    /// Calculates what timestamp the engine should be at based on the wall clock.
    pub fn get_expected_time(&self) -> Duration {
        if self.is_playing {
            if let Some(start) = self.start_time {
                return self.playback_offset + start.elapsed();
            }
        }
        self.playback_offset
    }

    pub fn current_ms(&self) -> i64 {
        self.get_expected_time().as_millis() as i64
    }
}
