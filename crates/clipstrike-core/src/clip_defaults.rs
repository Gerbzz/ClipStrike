pub fn timeline_clip_duration_ms(asset_duration_ms: u64, min_duration_ms: i64) -> i64 {
    let min_duration_ms = min_duration_ms.max(1);
    let asset_duration_ms = (asset_duration_ms.min(i64::MAX as u64)) as i64;
    asset_duration_ms.max(min_duration_ms)
}

pub fn timeline_clip_end_ms(start_ms: i64, asset_duration_ms: u64, min_duration_ms: i64) -> i64 {
    start_ms.saturating_add(timeline_clip_duration_ms(
        asset_duration_ms,
        min_duration_ms,
    ))
}

pub fn clamp_trim_to_asset_bounds(
    new_start: i64,
    new_end: i64,
    trim_left: bool,
    asset_duration_ms: u64,
) -> (i64, i64) {
    let max_duration_ms = (asset_duration_ms.min(i64::MAX as u64)) as i64;
    if max_duration_ms <= 0 {
        return (new_start, new_end.max(new_start + 1));
    }

    let duration_ms = (new_end - new_start).max(1);
    if duration_ms <= max_duration_ms {
        return (new_start, new_end.max(new_start + 1));
    }

    if trim_left {
        (new_end - max_duration_ms, new_end.max(new_start + 1))
    } else {
        (new_start, new_start.saturating_add(max_duration_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_real_asset_duration_when_available() {
        assert_eq!(timeline_clip_duration_ms(742, 33), 742);
    }

    #[test]
    fn falls_back_to_minimum_frame_when_duration_is_zero() {
        assert_eq!(timeline_clip_duration_ms(0, 33), 33);
    }

    #[test]
    fn saturates_end_time() {
        assert_eq!(timeline_clip_end_ms(i64::MAX - 5, 100, 33), i64::MAX);
    }

    #[test]
    fn left_trim_cannot_extend_past_source_duration() {
        assert_eq!(clamp_trim_to_asset_bounds(-500, 4_000, true, 4_000), (0, 4_000));
    }

    #[test]
    fn right_trim_cannot_extend_past_source_duration() {
        assert_eq!(clamp_trim_to_asset_bounds(1_000, 6_500, false, 4_000), (1_000, 5_000));
    }
}
