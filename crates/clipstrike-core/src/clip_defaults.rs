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
}
