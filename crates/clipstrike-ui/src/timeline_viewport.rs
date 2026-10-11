// Adapted from FilmCraft's crates/ui-egui/src/panels/timeline.rs at
// 2ff3f5e266838700c292a35aaf762e3d4dcef980 (Layout::x_of/tick_at,
// timeline_extent/max_scroll, and zoom_about). Copyright (c) 2026 ArtCraft
// Team and the FilmCraft contributors. Used under the MIT license; see
// docs/third_party/filmcraft/README.md and LICENSE-MIT there.
// Changes: ClipStrike uses integer milliseconds, a fixed-width egui drawing
// window, finite 1..=2,000 points/second bounds, and sequence-end clamping.

/// Pixel/time transform for the editor timeline. View state is transient UI
/// state; it is never written into a `.cstrike` project.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineViewport {
    pps: f64,
    scroll_seconds: f64,
}

impl TimelineViewport {
    pub const MIN_PPS: f64 = 1.0;
    pub const MAX_PPS: f64 = 2_000.0;

    pub fn new(pps: f64, scroll_seconds: f64) -> Self {
        Self {
            pps: if pps.is_finite() {
                pps.clamp(Self::MIN_PPS, Self::MAX_PPS)
            } else {
                50.0
            },
            scroll_seconds: if scroll_seconds.is_finite() {
                scroll_seconds.max(0.0)
            } else {
                0.0
            },
        }
    }

    pub fn pixels_per_second(&self) -> f64 {
        self.pps
    }
    pub fn pixels_per_ms(&self) -> f32 {
        (self.pps / 1_000.0) as f32
    }
    pub fn scroll_seconds(&self) -> f64 {
        self.scroll_seconds
    }
    /// FilmCraft's `Layout::x_of`, mapped from ticks to ClipStrike milliseconds.
    pub fn x_of_ms(&self, content_left: f32, time_ms: i64) -> f32 {
        content_left + ((time_ms as f64 / 1_000.0 - self.scroll_seconds) * self.pps) as f32
    }

    /// FilmCraft's `Layout::tick_at`, mapped to nonnegative milliseconds.
    pub fn ms_at_x(&self, content_left: f32, x: f32) -> i64 {
        let seconds = self.scroll_seconds + (x - content_left) as f64 / self.pps;
        (seconds * 1_000.0).round().clamp(0.0, i64::MAX as f64) as i64
    }

    pub fn visible_ms(&self, content_left: f32, screen_left: f32, screen_right: f32) -> (i64, i64) {
        (
            self.ms_at_x(content_left, screen_left),
            self.ms_at_x(content_left, screen_right),
        )
    }

    /// FilmCraft's timeline extent: leave ten minutes of navigable tail.
    pub fn timeline_extent(sequence_end_ms: i64) -> f64 {
        (sequence_end_ms.max(0) as f64 / 1_000.0) + 600.0
    }

    pub fn max_scroll(sequence_end_ms: i64, width: f32, pps: f64) -> f64 {
        if pps <= 0.0 || !pps.is_finite() || !width.is_finite() {
            return 0.0;
        }
        (Self::timeline_extent(sequence_end_ms) - width.max(0.0) as f64 / pps).max(0.0)
    }

    /// The drawing canvas stays local to the visible view. Long-project
    /// position is held in f64 seconds, never in an enormous egui f32 offset.
    pub fn content_width(&self, _sequence_end_ms: i64, viewport_width: f32) -> f32 {
        if viewport_width.is_finite() {
            viewport_width.max(1.0)
        } else {
            1.0
        }
    }

    pub fn ruler_step_ms(&self, min_spacing_px: f32) -> i64 {
        const STEPS: [i64; 13] = [
            50, 100, 250, 500, 1_000, 2_000, 5_000, 10_000, 30_000, 60_000, 120_000, 300_000,
            600_000,
        ];
        STEPS
            .into_iter()
            .find(|step| *step as f64 / 1_000.0 * self.pps >= min_spacing_px as f64)
            .unwrap_or(600_000)
    }

    /// FilmCraft's cursor-anchored `zoom_about`, with ClipStrike's zoom bounds.
    pub fn zoom_about(&mut self, factor: f64, anchor_ms: i64, width: f32, sequence_end_ms: i64) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let width = if width.is_finite() {
            width.max(0.0)
        } else {
            0.0
        };
        let old = self.pps;
        let new = (old * factor).clamp(Self::MIN_PPS, Self::MAX_PPS);
        let anchor_secs = anchor_ms.max(0) as f64 / 1_000.0;
        let x = ((anchor_secs - self.scroll_seconds) * old) as f32;
        let x = if (0.0..=width).contains(&x) {
            x
        } else {
            width / 2.0
        };
        self.pps = new;
        self.scroll_seconds = (anchor_secs - x as f64 / new)
            .clamp(0.0, Self::max_scroll(sequence_end_ms, width, new));
    }

    pub fn pan_pixels(&mut self, delta_x: f32, width: f32, sequence_end_ms: i64) {
        if delta_x.is_finite() {
            self.scroll_seconds = (self.scroll_seconds + delta_x as f64 / self.pps)
                .clamp(0.0, Self::max_scroll(sequence_end_ms, width, self.pps));
        }
    }

    pub fn center_on(&mut self, time_ms: i64, width: f32, sequence_end_ms: i64) {
        let width = if width.is_finite() {
            width.max(0.0)
        } else {
            0.0
        };
        self.scroll_seconds = (time_ms.max(0) as f64 / 1_000.0 - width as f64 / (2.0 * self.pps))
            .clamp(0.0, Self::max_scroll(sequence_end_ms, width, self.pps));
    }
}

#[cfg(test)]
mod tests {
    use super::TimelineViewport;

    #[test]
    fn screen_and_millisecond_coordinates_are_inverses_after_scroll() {
        let view = TimelineViewport::new(100.0, 2.25);
        let origin = 220.0;
        for time_ms in [2_250, 2_500, 3_000, 12_345] {
            let x = view.x_of_ms(origin, time_ms);
            assert!((view.ms_at_x(origin, x) - time_ms).abs() <= 1);
        }
        assert_eq!(view.ms_at_x(origin, origin - 100.0), 1_250);
    }

    #[test]
    fn zoom_keeps_pointer_time_at_the_same_screen_coordinate() {
        let mut view = TimelineViewport::new(50.0, 2.0);
        let origin = 300.0;
        let anchor_ms = 5_000;
        let x_before = view.x_of_ms(origin, anchor_ms);
        view.zoom_about(2.0, anchor_ms, 800.0, 20_000);
        let x_after = view.x_of_ms(origin, anchor_ms);
        assert!((x_after - x_before).abs() < 0.01);
        assert_eq!(view.pixels_per_second(), 100.0);
    }

    #[test]
    fn pan_and_extent_are_clamped_and_finite() {
        let mut view = TimelineViewport::new(50.0, 0.0);
        view.pan_pixels(-1_000.0, 800.0, 1_000);
        assert_eq!(view.scroll_seconds(), 0.0);
        view.pan_pixels(10_000_000.0, 800.0, 1_000);
        assert_eq!(
            view.scroll_seconds(),
            TimelineViewport::max_scroll(1_000, 800.0, 50.0)
        );
        view.zoom_about(0.0, 0, 800.0, 1_000);
        assert!(view.pixels_per_second().is_finite());
        assert!(view.pixels_per_second() >= 1.0);
    }

    #[test]
    fn adjacent_trimmed_clips_have_stable_visible_positions() {
        let view = TimelineViewport::new(100.0, 0.25);
        let origin = 200.0;
        assert_eq!(view.visible_ms(origin, origin, origin + 60.0), (250, 850));
        assert_eq!(view.x_of_ms(origin, 0), 175.0);
        assert_eq!(view.x_of_ms(origin, 500), 225.0);
        assert_eq!(view.x_of_ms(origin, 1_000), 275.0);
    }

    #[test]
    fn ruler_ticks_stay_legible_across_zoom_extremes() {
        assert_eq!(TimelineViewport::new(2_000.0, 0.0).ruler_step_ms(80.0), 50);
        assert_eq!(TimelineViewport::new(100.0, 0.0).ruler_step_ms(80.0), 1_000);
        assert_eq!(TimelineViewport::new(1.0, 0.0).ruler_step_ms(80.0), 120_000);
    }

    #[test]
    fn late_clip_at_max_zoom_is_reachable_in_a_local_coordinate_window() {
        let late_ms = 3 * 60 * 60 * 1_000;
        let mut view = TimelineViewport::new(TimelineViewport::MAX_PPS, 0.0);
        let width = 800.0;
        view.center_on(late_ms, width, late_ms + 1_000);
        let x = view.x_of_ms(200.0, late_ms);
        assert!((x - 600.0).abs() <= 0.01);
        assert!((view.ms_at_x(200.0, x) - late_ms).abs() <= 1);
        assert_eq!(view.content_width(late_ms + 1_000, width), width);
    }

    #[test]
    fn non_finite_widths_do_not_corrupt_view_state() {
        let mut view = TimelineViewport::new(100.0, 2.0);
        for width in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            view.zoom_about(1.5, 4_000, width, 10_000);
            assert!(view.scroll_seconds().is_finite());
            assert!(view.pixels_per_second().is_finite());
            view.center_on(8_000, width, 10_000);
            assert!(view.scroll_seconds().is_finite());
            assert!(view.content_width(10_000, width).is_finite());
        }
    }
}
