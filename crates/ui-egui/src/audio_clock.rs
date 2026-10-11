//! A smooth picture clock that follows the sound device without exposing its callback steps.
//!
//! Audio devices report a new played-frame count once per callback, often about every 10 ms.
//! Sampling that stepped value from a 60 Hz UI can repeatedly skip every third picture frame
//! for some callback phases. Wall time supplies the between-callback motion; the device clock
//! corrects long-term drift outside one callback of quantization noise.

const CALLBACK_JITTER_S: f64 = 0.012;
const MAX_CORRECTION_PER_FRAME_S: f64 = 0.0005;

#[derive(Clone, Debug, Default)]
pub struct SmoothAudioClock {
    correction_s: f64,
}

impl SmoothAudioClock {
    pub fn reset(&mut self) {
        self.correction_s = 0.0;
    }

    pub fn elapsed(&mut self, wall_s: f64, device_s: f64) -> f64 {
        if !wall_s.is_finite() || !device_s.is_finite() {
            return wall_s.max(0.0);
        }
        let error = device_s - (wall_s + self.correction_s);
        if error > CALLBACK_JITTER_S {
            self.correction_s += (error - CALLBACK_JITTER_S).min(MAX_CORRECTION_PER_FRAME_S);
        } else if error < -CALLBACK_JITTER_S {
            self.correction_s += (error + CALLBACK_JITTER_S).max(-MAX_CORRECTION_PER_FRAME_S);
        }
        (wall_s + self.correction_s).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_millisecond_device_callbacks_do_not_skip_sixty_fps_picture_frames() {
        for phase_ms in [0.0, 1.0, 3.0, 5.0, 7.0, 9.0] {
            let mut clock = SmoothAudioClock::default();
            let (mut previous, mut skipped): (Option<i64>, i64) = (None, 0);
            for n in 0..180 {
                let wall = n as f64 / 60.0;
                let stepped_device = ((wall + phase_ms / 1000.0) / 0.01).floor() * 0.01;
                let frame = (clock.elapsed(wall, stepped_device) * 60.0 + 1e-8).floor() as i64;
                if let Some(last) = previous {
                    skipped += (frame - last - 1).max(0);
                }
                previous = Some(frame);
            }
            assert_eq!(skipped, 0, "callback phase {phase_ms} ms skipped frames");
        }
    }

    #[test]
    fn correction_follows_long_term_device_drift_without_callback_jitter() {
        let mut clock = SmoothAudioClock::default();
        let mut shown = 0.0;
        for n in 0..=3600 {
            let wall = n as f64 / 60.0;
            let device = (wall * 1.01 / 0.01).floor() * 0.01;
            shown = clock.elapsed(wall, device);
        }
        assert!((shown - 60.6).abs() < 0.03, "picture drifted away from the 1% faster audio device: {shown}");
        clock.reset();
        assert_eq!(clock.elapsed(0.0, 0.0), 0.0);
    }
}
