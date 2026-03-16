use serde::{Deserialize, Serialize};

pub const DEFAULT_KEYFRAME_SNAP_MS: i64 = 100;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum InterpolationMode {
    Linear,
    Bezier,
    Constant,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Keyframe {
    pub time_ms: i64,
    pub value: f32,
    pub mode: InterpolationMode,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct KeyframedProperty {
    pub keyframes: Vec<Keyframe>,
    pub default_value: f32,
}

impl KeyframedProperty {
    pub fn new(default_value: f32) -> Self {
        Self {
            keyframes: Vec::new(),
            default_value,
        }
    }

    /// Task 9.2, 9.3, 9.4: Calculate interpolated value at a specific time.
    pub fn get_value_at(&self, time_ms: i64) -> f32 {
        if self.keyframes.is_empty() {
            return self.default_value;
        }

        let mut sorted = self.keyframes.clone();
        sorted.sort_by_key(|k| k.time_ms);

        if time_ms <= sorted[0].time_ms {
            return sorted[0].value;
        }

        if time_ms >= sorted.last().unwrap().time_ms {
            return sorted.last().unwrap().value;
        }

        for i in 0..sorted.len() - 1 {
            let k1 = &sorted[i];
            let k2 = &sorted[i + 1];

            if time_ms >= k1.time_ms && time_ms <= k2.time_ms {
                let range = k2.time_ms - k1.time_ms;
                if range == 0 {
                    return k1.value;
                }

                let progress = (time_ms - k1.time_ms) as f32 / range as f32;

                return match k1.mode {
                    InterpolationMode::Constant => k1.value,
                    InterpolationMode::Linear => k1.value + (k2.value - k1.value) * progress,
                    InterpolationMode::Bezier => {
                        let control = if k2.value >= k1.value {
                            k1.value + (k2.value - k1.value) * 0.8
                        } else {
                            k1.value - (k1.value - k2.value) * 0.8
                        };
                        let one_minus_t = 1.0 - progress;
                        one_minus_t * one_minus_t * k1.value
                            + 2.0 * one_minus_t * progress * control
                            + progress * progress * k2.value
                    }
                };
            }
        }

        self.default_value
    }

    pub fn insert_keyframe(&mut self, time_ms: i64, value: f32, mode: InterpolationMode) {
        if let Some(existing) = self.keyframes.iter_mut().find(|k| k.time_ms == time_ms) {
            existing.value = value;
            existing.mode = mode;
        } else {
            self.keyframes.push(Keyframe {
                time_ms,
                value,
                mode,
            });
            self.keyframes.sort_by_key(|k| k.time_ms);
        }
    }

    pub fn insert_keyframe_snapped(
        &mut self,
        time_ms: i64,
        value: f32,
        mode: InterpolationMode,
        snap_threshold_ms: i64,
        extra_snap_points: &[i64],
    ) -> i64 {
        let snapped_time = self.snap_time(time_ms, snap_threshold_ms, extra_snap_points, None);
        self.insert_keyframe(snapped_time, value, mode);
        snapped_time
    }

    pub fn remove_keyframe(&mut self, time_ms: i64) -> Option<Keyframe> {
        let position = self
            .keyframes
            .iter()
            .position(|keyframe| keyframe.time_ms == time_ms)?;
        Some(self.keyframes.remove(position))
    }

    pub fn keyframe(&self, time_ms: i64) -> Option<&Keyframe> {
        self.keyframes
            .iter()
            .find(|keyframe| keyframe.time_ms == time_ms)
    }

    pub fn move_keyframe(
        &mut self,
        from_time_ms: i64,
        to_time_ms: i64,
        snap_threshold_ms: i64,
        extra_snap_points: &[i64],
    ) -> Result<i64, String> {
        let position = self
            .keyframes
            .iter()
            .position(|keyframe| keyframe.time_ms == from_time_ms)
            .ok_or_else(|| format!("No keyframe at {from_time_ms}ms"))?;
        let keyframe = self.keyframes.remove(position);
        let snapped_time = self.snap_time(
            to_time_ms,
            snap_threshold_ms,
            extra_snap_points,
            Some(from_time_ms),
        );
        self.insert_keyframe(snapped_time, keyframe.value, keyframe.mode);
        Ok(snapped_time)
    }

    pub fn update_keyframe(
        &mut self,
        time_ms: i64,
        value: f32,
        mode: InterpolationMode,
    ) -> Result<(), String> {
        let keyframe = self
            .keyframes
            .iter_mut()
            .find(|keyframe| keyframe.time_ms == time_ms)
            .ok_or_else(|| format!("No keyframe at {time_ms}ms"))?;
        keyframe.value = value;
        keyframe.mode = mode;
        Ok(())
    }

    pub fn snap_time(
        &self,
        proposed_time_ms: i64,
        snap_threshold_ms: i64,
        extra_snap_points: &[i64],
        exclude_time_ms: Option<i64>,
    ) -> i64 {
        let mut closest_time = proposed_time_ms;
        let mut closest_distance = snap_threshold_ms.saturating_add(1);

        for candidate in extra_snap_points
            .iter()
            .copied()
            .chain(self.keyframes.iter().map(|keyframe| keyframe.time_ms))
        {
            if Some(candidate) == exclude_time_ms {
                continue;
            }
            let distance = (candidate - proposed_time_ms).abs();
            if distance <= snap_threshold_ms && distance < closest_distance {
                closest_distance = distance;
                closest_time = candidate;
            }
        }

        closest_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_across_multiple_segments() {
        let mut prop = KeyframedProperty::new(0.0);
        prop.insert_keyframe(0, 0.0, InterpolationMode::Linear);
        prop.insert_keyframe(1000, 10.0, InterpolationMode::Linear);
        prop.insert_keyframe(2000, 30.0, InterpolationMode::Linear);

        assert_eq!(prop.get_value_at(-1), 0.0);
        assert!((prop.get_value_at(500) - 5.0).abs() < f32::EPSILON);
        assert!((prop.get_value_at(1500) - 20.0).abs() < f32::EPSILON);
        assert_eq!(prop.get_value_at(2500), 30.0);
    }

    #[test]
    fn bezier_mode_curves_between_keyframes() {
        let mut prop = KeyframedProperty::new(0.0);
        prop.insert_keyframe(0, 0.0, InterpolationMode::Bezier);
        prop.insert_keyframe(1000, 10.0, InterpolationMode::Linear);

        let halfway = prop.get_value_at(500);
        assert!(halfway > 5.0);
        assert!(halfway < 10.0);
    }

    #[test]
    fn constant_mode_holds_until_next_keyframe() {
        let mut prop = KeyframedProperty::new(1.0);
        prop.insert_keyframe(0, 2.0, InterpolationMode::Constant);
        prop.insert_keyframe(1000, 8.0, InterpolationMode::Linear);

        assert_eq!(prop.get_value_at(500), 2.0);
    }

    #[test]
    fn insert_keyframe_updates_existing_timestamp() {
        let mut prop = KeyframedProperty::new(0.0);
        prop.insert_keyframe(100, 1.0, InterpolationMode::Linear);
        prop.insert_keyframe(100, 4.0, InterpolationMode::Bezier);

        assert_eq!(prop.keyframes.len(), 1);
        assert_eq!(prop.keyframes[0].value, 4.0);
        assert_eq!(prop.keyframes[0].mode, InterpolationMode::Bezier);
    }

    #[test]
    fn insert_keyframe_snaps_to_existing_keyframe_and_playhead() {
        let mut prop = KeyframedProperty::new(0.0);
        prop.insert_keyframe(1_000, 2.0, InterpolationMode::Linear);

        let snapped_to_existing =
            prop.insert_keyframe_snapped(1_040, 5.0, InterpolationMode::Constant, 50, &[]);
        assert_eq!(snapped_to_existing, 1_000);
        assert_eq!(prop.keyframes.len(), 1);
        assert_eq!(prop.keyframes[0].value, 5.0);

        let snapped_to_playhead =
            prop.insert_keyframe_snapped(1_940, 7.0, InterpolationMode::Linear, 75, &[2_000]);
        assert_eq!(snapped_to_playhead, 2_000);
        assert!(prop.keyframe(2_000).is_some());
    }

    #[test]
    fn move_keyframe_snaps_without_colliding_with_itself() {
        let mut prop = KeyframedProperty::new(0.0);
        prop.insert_keyframe(1_000, 1.0, InterpolationMode::Linear);
        prop.insert_keyframe(2_000, 2.0, InterpolationMode::Bezier);

        let new_time = prop
            .move_keyframe(1_000, 1_960, 75, &[0, 2_000])
            .expect("move should succeed");

        assert_eq!(new_time, 2_000);
        assert_eq!(prop.keyframes.len(), 1);
        assert_eq!(prop.keyframes[0].time_ms, 2_000);
        assert_eq!(prop.keyframes[0].value, 1.0);
    }

    #[test]
    fn remove_and_update_keyframes_work() {
        let mut prop = KeyframedProperty::new(3.0);
        prop.insert_keyframe(500, 4.0, InterpolationMode::Linear);

        prop.update_keyframe(500, 6.5, InterpolationMode::Constant)
            .expect("update should succeed");
        let keyframe = prop.keyframe(500).expect("keyframe should exist");
        assert_eq!(keyframe.value, 6.5);
        assert_eq!(keyframe.mode, InterpolationMode::Constant);

        let removed = prop.remove_keyframe(500).expect("remove should succeed");
        assert_eq!(removed.time_ms, 500);
        assert!(prop.keyframes.is_empty());
        assert_eq!(prop.get_value_at(500), 3.0);
    }
}
