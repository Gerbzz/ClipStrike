pub fn clip_instance_suffix(clip_id: usize) -> String {
    format!("C{:03}", clip_id % 1000)
}

pub fn timeline_clip_label(asset_name: &str, clip_id: usize, max_chars: usize) -> String {
    let suffix = format!(" [{}]", clip_instance_suffix(clip_id));
    let suffix_len = suffix.chars().count();
    if max_chars <= suffix_len {
        return clip_instance_suffix(clip_id);
    }

    let available_name_chars = max_chars.saturating_sub(suffix_len);
    let asset_chars = asset_name.chars().count();
    let display_name = if asset_chars <= available_name_chars {
        asset_name.to_string()
    } else if available_name_chars <= 3 {
        String::new()
    } else {
        let truncated: String = asset_name.chars().take(available_name_chars - 3).collect();
        format!("{}...", truncated)
    };

    if display_name.is_empty() {
        clip_instance_suffix(clip_id)
    } else {
        format!("{}{}", display_name, suffix)
    }
}

pub fn timeline_clip_tooltip(asset_name: &str, clip_id: usize) -> String {
    format!("{} [{}]", asset_name, clip_instance_suffix(clip_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_asset_name_when_it_fits() {
        assert_eq!(timeline_clip_label("match.mp4", 42, 32), "match.mp4 [C042]");
    }

    #[test]
    fn truncates_name_but_keeps_suffix() {
        assert_eq!(
            timeline_clip_label("very_long_clip_name.mp4", 42, 18),
            "very_lon... [C042]"
        );
    }

    #[test]
    fn falls_back_to_suffix_when_space_is_tiny() {
        assert_eq!(timeline_clip_label("match.mp4", 42, 4), "C042");
    }
}
