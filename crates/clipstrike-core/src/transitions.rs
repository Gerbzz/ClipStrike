use crate::keyframes::KeyframedProperty;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransitionType {
    Crossfade,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    WipeDiagonal,
    WipeClock,
    RadialWipe,
    MatrixWipe,
    SpiralWipe,
    CustomMask,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transition {
    pub id: usize,
    pub track_id: usize,
    pub start_ms: i64,
    pub end_ms: i64,
    pub transition_type: TransitionType,
    #[serde(default = "default_transition_speed")]
    pub speed: KeyframedProperty,
    #[serde(default = "default_transition_softness")]
    pub softness: KeyframedProperty,
    #[serde(default)]
    pub custom_mask_id: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalizedRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl NormalizedRect {
    pub const FULL: Self = Self {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 1.0,
        max_y: 1.0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionPreviewLayer {
    pub rect: NormalizedRect,
    pub uv: NormalizedRect,
    pub alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionPreviewLayout {
    pub outgoing: TransitionPreviewLayer,
    pub incoming: TransitionPreviewLayer,
}

#[derive(Debug, Clone, Copy)]
pub struct MaskImageRef<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

fn default_transition_speed() -> KeyframedProperty {
    KeyframedProperty::new(1.0)
}

fn default_transition_softness() -> KeyframedProperty {
    KeyframedProperty::new(0.05)
}

impl Transition {
    pub fn new(
        id: usize,
        track_id: usize,
        start_ms: i64,
        end_ms: i64,
        transition_type: TransitionType,
    ) -> Self {
        Self {
            id,
            track_id,
            start_ms,
            end_ms,
            transition_type,
            speed: default_transition_speed(),
            softness: default_transition_softness(),
            custom_mask_id: None,
        }
    }

    pub fn contains_time(&self, time_ms: i64) -> bool {
        time_ms >= self.start_ms && time_ms <= self.end_ms
    }

    pub fn duration_ms(&self) -> i64 {
        (self.end_ms - self.start_ms).max(1)
    }

    pub fn base_progress_at(&self, time_ms: i64) -> f32 {
        ((time_ms - self.start_ms) as f32 / self.duration_ms() as f32).clamp(0.0, 1.0)
    }

    pub fn progress_at(&self, time_ms: i64) -> f32 {
        let base = self.base_progress_at(time_ms);
        let speed = self.speed.get_value_at(time_ms).max(0.1);
        base.powf(1.0 / speed).clamp(0.0, 1.0)
    }

    pub fn softness_at(&self, time_ms: i64) -> f32 {
        self.softness.get_value_at(time_ms).clamp(0.0, 1.0)
    }
}

impl TransitionType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Crossfade => "Crossfade",
            Self::WipeLeft => "Wipe Left",
            Self::WipeRight => "Wipe Right",
            Self::WipeUp => "Wipe Up",
            Self::WipeDown => "Wipe Down",
            Self::WipeDiagonal => "Wipe Diagonal",
            Self::WipeClock => "Wipe Clock",
            Self::RadialWipe => "Radial Wipe",
            Self::MatrixWipe => "Matrix Wipe",
            Self::SpiralWipe => "Spiral Wipe",
            Self::CustomMask => "Custom Mask",
        }
    }

    pub fn uses_luma_mask(self) -> bool {
        !matches!(self, Self::Crossfade)
    }

    pub fn preview_layout(self, progress: f32) -> TransitionPreviewLayout {
        let progress = progress.clamp(0.0, 1.0);
        let full = TransitionPreviewLayer {
            rect: NormalizedRect::FULL,
            uv: NormalizedRect::FULL,
            alpha: 1.0,
        };

        match self {
            Self::Crossfade => TransitionPreviewLayout {
                outgoing: TransitionPreviewLayer {
                    alpha: 1.0 - progress,
                    ..full
                },
                incoming: TransitionPreviewLayer {
                    alpha: progress,
                    ..full
                },
            },
            _ => TransitionPreviewLayout {
                outgoing: full,
                incoming: full,
            },
        }
    }
}

pub fn sample_mask_value(
    transition_type: TransitionType,
    custom_mask: Option<MaskImageRef<'_>>,
    u: f32,
    v: f32,
) -> f32 {
    let u = u.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    match transition_type {
        TransitionType::Crossfade => u,
        TransitionType::WipeLeft => 1.0 - u,
        TransitionType::WipeRight => u,
        TransitionType::WipeUp => 1.0 - v,
        TransitionType::WipeDown => v,
        TransitionType::WipeDiagonal => ((u + v) * 0.5).clamp(0.0, 1.0),
        TransitionType::WipeClock => {
            let dx = u - 0.5;
            let dy = v - 0.5;
            let angle = (dx.atan2(-dy) + 2.0 * PI) % (2.0 * PI);
            (angle / (2.0 * PI)).clamp(0.0, 1.0)
        }
        TransitionType::RadialWipe => {
            let dx = u - 0.5;
            let dy = v - 0.5;
            let dist = (dx * dx + dy * dy).sqrt() * 2.0;
            dist.clamp(0.0, 1.0)
        }
        TransitionType::MatrixWipe => {
            let cols = 8.0;
            let rows = 6.0;
            let cu = (u * cols).floor() / cols;
            let cv = (v * rows).floor() / rows;
            ((cu + cv) * 0.5).clamp(0.0, 1.0)
        }
        TransitionType::SpiralWipe => {
            let dx = u - 0.5;
            let dy = v - 0.5;
            let dist = (dx * dx + dy * dy).sqrt();
            let angle = dx.atan2(dy);
            ((dist * 10.0 + angle / (2.0 * PI)) % 1.0).abs()
        }
        TransitionType::CustomMask => {
            let Some(mask) = custom_mask else {
                return u;
            };
            if mask.width == 0 || mask.height == 0 || mask.pixels.is_empty() {
                return u;
            }
            let x = (u * (mask.width.saturating_sub(1)) as f32).round() as usize;
            let y = (v * (mask.height.saturating_sub(1)) as f32).round() as usize;
            let index = y.saturating_mul(mask.width as usize).saturating_add(x);
            mask.pixels
                .get(index)
                .copied()
                .map(|value| value as f32 / 255.0)
                .unwrap_or(u)
        }
    }
}

pub fn incoming_mask_alpha(mask_value: f32, progress: f32, softness: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    let softness = softness.clamp(0.0, 1.0);
    if softness <= f32::EPSILON {
        return if mask_value <= progress { 1.0 } else { 0.0 };
    }

    let half = softness * 0.5;
    let lower = (progress - half).clamp(0.0, 1.0);
    let upper = (progress + half).clamp(0.0, 1.0);
    if upper <= lower {
        return if mask_value <= progress { 1.0 } else { 0.0 };
    }

    let t = ((mask_value - lower) / (upper - lower)).clamp(0.0, 1.0);
    let smoothstep = t * t * (3.0 - 2.0 * t);
    1.0 - smoothstep
}

pub fn default_fade_duration_ms(clip_start_ms: i64, clip_end_ms: i64) -> i64 {
    (clip_end_ms - clip_start_ms).max(1).min(1000)
}

pub fn fade_in_points(clip_start_ms: i64, clip_end_ms: i64) -> [(i64, f32); 2] {
    let duration = default_fade_duration_ms(clip_start_ms, clip_end_ms);
    [
        (clip_start_ms, 0.0),
        ((clip_start_ms + duration).min(clip_end_ms), 1.0),
    ]
}

pub fn fade_out_points(clip_start_ms: i64, clip_end_ms: i64) -> [(i64, f32); 2] {
    let duration = default_fade_duration_ms(clip_start_ms, clip_end_ms);
    [
        ((clip_end_ms - duration).max(clip_start_ms), 1.0),
        (clip_end_ms, 0.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframes::InterpolationMode;

    #[test]
    fn transition_progress_is_clamped() {
        let transition = Transition::new(1, 1, 100, 300, TransitionType::Crossfade);
        assert_eq!(transition.progress_at(0), 0.0);
        assert!((transition.progress_at(200) - 0.5).abs() < f32::EPSILON);
        assert_eq!(transition.progress_at(400), 1.0);
    }

    #[test]
    fn transition_speed_shapes_progress() {
        let mut transition = Transition::new(1, 1, 0, 1000, TransitionType::Crossfade);
        transition
            .speed
            .insert_keyframe(500, 4.0, InterpolationMode::Linear);
        assert!(transition.progress_at(500) > transition.base_progress_at(500));
    }

    #[test]
    fn crossfade_layout_uses_complementary_alpha() {
        let layout = TransitionType::Crossfade.preview_layout(0.25);
        assert_eq!(layout.outgoing.rect, NormalizedRect::FULL);
        assert_eq!(layout.incoming.rect, NormalizedRect::FULL);
        assert!((layout.outgoing.alpha - 0.75).abs() < f32::EPSILON);
        assert!((layout.incoming.alpha - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn wipe_diagonal_increases_toward_bottom_right() {
        let top_left = sample_mask_value(TransitionType::WipeDiagonal, None, 0.1, 0.1);
        let bottom_right = sample_mask_value(TransitionType::WipeDiagonal, None, 0.9, 0.9);
        assert!(top_left < bottom_right);
    }

    #[test]
    fn clock_wipe_starts_from_top() {
        let top = sample_mask_value(TransitionType::WipeClock, None, 0.5, 0.0);
        let right = sample_mask_value(TransitionType::WipeClock, None, 1.0, 0.5);
        assert!(top < right);
        assert!(top.abs() < f32::EPSILON || top < 0.05);
        assert!((right - 0.25).abs() < 0.05);
    }

    #[test]
    fn custom_mask_uses_imported_pixels() {
        let pixels = [0u8, 255, 255, 0];
        let mask = MaskImageRef {
            width: 2,
            height: 2,
            pixels: &pixels,
        };
        assert!(sample_mask_value(TransitionType::CustomMask, Some(mask), 0.0, 0.0) < 0.01);
        assert!(sample_mask_value(TransitionType::CustomMask, Some(mask), 1.0, 0.0) > 0.99);
    }

    #[test]
    fn softness_blends_mask_edges() {
        let hard = incoming_mask_alpha(0.55, 0.5, 0.0);
        let soft = incoming_mask_alpha(0.55, 0.5, 0.2);
        assert_eq!(hard, 0.0);
        assert!(soft > 0.0 && soft < 1.0);
    }

    #[test]
    fn fade_points_clamp_to_clip_bounds() {
        assert_eq!(fade_in_points(100, 300), [(100, 0.0), (300, 1.0)]);
        assert_eq!(fade_out_points(100, 300), [(100, 1.0), (300, 0.0)]);
        assert_eq!(fade_in_points(0, 4000), [(0, 0.0), (1000, 1.0)]);
        assert_eq!(fade_out_points(0, 4000), [(3000, 1.0), (4000, 0.0)]);
    }
}
