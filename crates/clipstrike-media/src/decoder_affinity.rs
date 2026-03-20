pub fn hardware_decoder_candidates_for(target_os: &str) -> &'static [&'static str] {
    match target_os {
        "windows" => &["d3d11h264dec", "d3d11h265dec", "d3d11vp9dec", "d3d11av1dec"],
        "linux" => &[
            "vaapih264dec",
            "vaapih265dec",
            "nvh264dec",
            "nvh265dec",
            "nvvp8dec",
            "nvvp9dec",
        ],
        "macos" => &["vtdec"],
        _ => &[],
    }
}

#[cfg(feature = "gstreamer-backend")]
pub fn prefer_hardware_decoders() {
    use gstreamer as gst;
    use gstreamer::prelude::PluginFeatureExtManual;

    let registry = gst::Registry::get();
    for decoder_name in hardware_decoder_candidates_for(std::env::consts::OS) {
        if let Some(feature) = registry.lookup_feature(decoder_name) {
            feature.set_rank(gst::Rank::PRIMARY);
            log::debug!("Preferring hardware decoder feature: {}", decoder_name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::hardware_decoder_candidates_for;

    #[test]
    fn hardware_decoder_candidates_prefer_d3d11_on_windows() {
        let candidates = hardware_decoder_candidates_for("windows");
        assert_eq!(candidates[0], "d3d11h264dec");
        assert!(candidates.contains(&"d3d11h265dec"));
    }

    #[test]
    fn hardware_decoder_candidates_include_vaapi_on_linux() {
        let candidates = hardware_decoder_candidates_for("linux");
        assert!(candidates.contains(&"vaapih264dec"));
        assert!(candidates.contains(&"nvh264dec"));
    }
}
