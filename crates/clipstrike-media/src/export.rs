use serde::{Deserialize, Serialize};
use std::path::Path;
#[cfg(feature = "gstreamer-backend")]
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExportFormat {
    MP4,
    WebM,
    MOV,
    MKV,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    VP9,
    AV1,
    ProRes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportSettings {
    pub format: ExportFormat,
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub fps_num: i32,
    pub fps_den: i32,
    pub bitrate_kbps: u32,
    pub use_hardware_acceleration: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            format: ExportFormat::MP4,
            codec: VideoCodec::H264,
            width: 1920,
            height: 1080,
            fps_num: 30,
            fps_den: 1,
            bitrate_kbps: 8000,
            use_hardware_acceleration: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportPreset {
    pub name: String,
    pub description: String,
    pub settings: ExportSettings,
}

impl ExportPreset {
    pub fn youtube_1080p() -> Self {
        Self {
            name: "YouTube 1080p".to_string(),
            description: "High quality H.264, 1080p, 30fps".to_string(),
            settings: ExportSettings {
                format: ExportFormat::MP4,
                codec: VideoCodec::H264,
                width: 1920,
                height: 1080,
                fps_num: 30,
                fps_den: 1,
                bitrate_kbps: 10000,
                use_hardware_acceleration: true,
            },
        }
    }

    pub fn ultra_hd_4k() -> Self {
        Self {
            name: "4K Ultra HD".to_string(),
            description: "Maximum quality H.264, 4K, 60fps".to_string(),
            settings: ExportSettings {
                format: ExportFormat::MP4,
                codec: VideoCodec::H264,
                width: 3840,
                height: 2160,
                fps_num: 60,
                fps_den: 1,
                bitrate_kbps: 40000,
                use_hardware_acceleration: true,
            },
        }
    }

    pub fn mobile_social() -> Self {
        Self {
            name: "Mobile/Social".to_string(),
            description: "Optimized for mobile devices, 720p".to_string(),
            settings: ExportSettings {
                format: ExportFormat::MP4,
                codec: VideoCodec::H264,
                width: 720,
                height: 1280,
                fps_num: 30,
                fps_den: 1,
                bitrate_kbps: 5000,
                use_hardware_acceleration: true,
            },
        }
    }
}

#[cfg(feature = "gstreamer-backend")]
fn muxer_name_for_format(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::MP4 => "mp4mux",
        ExportFormat::WebM => "webmmux",
        ExportFormat::MOV => "qtmux",
        ExportFormat::MKV => "matroskamux",
    }
}

#[cfg(feature = "gstreamer-backend")]
fn parser_name_for_codec(codec: VideoCodec) -> Option<&'static str> {
    match codec {
        VideoCodec::H264 => Some("h264parse"),
        VideoCodec::H265 => Some("h265parse"),
        VideoCodec::VP9 => Some("vp9parse"),
        VideoCodec::AV1 => Some("av1parse"),
        VideoCodec::ProRes => None,
    }
}

#[cfg(feature = "gstreamer-backend")]
fn encoder_candidates_for_settings(settings: &ExportSettings) -> &'static [&'static str] {
    match (settings.codec, settings.use_hardware_acceleration) {
        (VideoCodec::H264, true) => &[
            "nvh264enc",
            "vaapih264enc",
            "qsvh264enc",
            "openh264enc",
            "x264enc",
        ],
        (VideoCodec::H264, false) => &["x264enc", "openh264enc"],
        (VideoCodec::H265, true) => &["nvh265enc", "vaapih265enc", "qsvh265enc", "x265enc"],
        (VideoCodec::H265, false) => &["x265enc"],
        (VideoCodec::VP9, _) => &["vp9enc"],
        (VideoCodec::AV1, _) => &["svtav1enc", "rav1enc", "av1enc"],
        (VideoCodec::ProRes, _) => &["proresenc_aw", "avenc_prores_ks", "avenc_prores"],
    }
}

#[cfg(feature = "gstreamer-backend")]
fn configure_encoder_properties(
    encoder_name: &str,
    encoder: &gstreamer::Element,
    settings: &ExportSettings,
) {
    use gstreamer::prelude::*;

    match settings.codec {
        VideoCodec::H264 => {
            if encoder_name == "x264enc" {
                encoder.set_property_from_str("tune", "zerolatency");
                encoder.set_property("bitrate", settings.bitrate_kbps);
                encoder.set_property("key-int-max", 30u32);
            } else {
                let _ = encoder.set_property("bitrate", settings.bitrate_kbps);
            }
        }
        VideoCodec::H265 => {
            let _ = encoder.set_property("bitrate", settings.bitrate_kbps);
        }
        VideoCodec::VP9 | VideoCodec::AV1 | VideoCodec::ProRes => {
            let _ = encoder.set_property("bitrate", settings.bitrate_kbps);
        }
    }
}

#[cfg(feature = "gstreamer-backend")]
mod imp {
    use super::{
        ExportSettings, Path, PathBuf, configure_encoder_properties,
        encoder_candidates_for_settings, muxer_name_for_format, parser_name_for_codec,
    };
    use gst::prelude::*;
    use gstreamer as gst;
    use gstreamer_app as gst_app;
    use gstreamer_video as gst_video;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// The Export Pipeline for rendering the project to a file.
    pub struct Exporter {
        pipeline: gst::Pipeline,
        appsrc: gst_app::AppSrc,
        width: u32,
        height: u32,
        fps_num: i32,
        fps_den: i32,
        next_frame_index: AtomicU64,
        _output_path: PathBuf,
    }

    impl Exporter {
        pub fn new<P: AsRef<Path>>(
            output_path: P,
            settings: &ExportSettings,
        ) -> Result<Self, String> {
            gst::init().map_err(|e| e.to_string())?;

            let pipeline = gst::Pipeline::with_name("export_pipeline");
            let appsrc = gst::ElementFactory::make("appsrc")
                .build()
                .map_err(|e| e.to_string())?;
            let videoconvert = gst::ElementFactory::make("videoconvert")
                .build()
                .map_err(|e| e.to_string())?;
            let encoder_candidates = encoder_candidates_for_settings(settings);
            let (encoder, encoder_name) =
                make_first_available_element(encoder_candidates, "video encoder")?;
            configure_encoder_properties(&encoder_name, &encoder, settings);
            let parser = parser_name_for_codec(settings.codec)
                .map(|name| {
                    gst::ElementFactory::make(name)
                        .build()
                        .map_err(|e| e.to_string())
                })
                .transpose()?;
            let muxer = gst::ElementFactory::make(muxer_name_for_format(settings.format))
                .build()
                .map_err(|e| e.to_string())?;
            let filesink = gst::ElementFactory::make("filesink")
                .build()
                .map_err(|e| e.to_string())?;

            filesink.set_property("location", output_path.as_ref().to_str().unwrap());

            let video_info = gst_video::VideoInfo::builder(
                gst_video::VideoFormat::Rgba,
                settings.width,
                settings.height,
            )
            .fps(gst::Fraction::new(settings.fps_num, settings.fps_den))
            .build()
            .map_err(|e: glib::error::BoolError| e.to_string())?;

            let appsrc = appsrc
                .downcast::<gst_app::AppSrc>()
                .map_err(|_| "Failed to cast appsrc".to_string())?;
            appsrc.set_caps(Some(
                &video_info
                    .to_caps()
                    .map_err(|e: glib::error::BoolError| e.to_string())?,
            ));
            appsrc.set_format(gst::Format::Time);
            // appsrc duration is the duration of the whole stream, not one frame.
            let mut elements: Vec<&gst::Element> =
                vec![appsrc.upcast_ref(), &videoconvert, &encoder];
            if let Some(parser) = &parser {
                elements.push(parser);
            }
            elements.push(&muxer);
            elements.push(&filesink);

            pipeline.add_many(&elements).map_err(|e| e.to_string())?;

            gst::Element::link_many(&elements).map_err(|e| e.to_string())?;

            Ok(Self {
                pipeline,
                appsrc,
                width: settings.width,
                height: settings.height,
                fps_num: settings.fps_num,
                fps_den: settings.fps_den,
                next_frame_index: AtomicU64::new(0),
                _output_path: output_path.as_ref().to_path_buf(),
            })
        }

        pub fn start(&self) -> Result<(), String> {
            self.pipeline
                .set_state(gst::State::Playing)
                .map_err(|e| e.to_string())?;
            Ok(())
        }

        pub fn push_frame(&self, bytes: &[u8]) -> Result<(), String> {
            let mut buffer = gst::Buffer::with_size(bytes.len())
                .map_err(|_| "Failed to allocate export buffer".to_string())?;
            let frame_index = self.next_frame_index.fetch_add(1, Ordering::Relaxed);
            let pts_ns = frame_pts_nanos(frame_index, self.fps_num, self.fps_den);
            let next_pts_ns = frame_pts_nanos(frame_index + 1, self.fps_num, self.fps_den);
            let frame_duration_ns = next_pts_ns.saturating_sub(pts_ns);
            {
                let buffer_mut = buffer
                    .get_mut()
                    .ok_or_else(|| "Failed to get mutable export buffer".to_string())?;
                buffer_mut.set_pts(Some(gst::ClockTime::from_nseconds(pts_ns)));
                buffer_mut.set_duration(Some(gst::ClockTime::from_nseconds(frame_duration_ns)));
                let mut map = buffer_mut
                    .map_writable()
                    .map_err(|_| "Failed to map export buffer".to_string())?;
                map.copy_from_slice(bytes);
            }
            self.appsrc.push_buffer(buffer).map_err(|e| e.to_string())?;
            Ok(())
        }

        pub fn end_of_stream(&self) -> Result<(), String> {
            self.appsrc.end_of_stream().map_err(|e| e.to_string())?;
            let bus = self
                .pipeline
                .bus()
                .ok_or_else(|| "Export pipeline missing bus".to_string())?;
            let message = bus
                .timed_pop_filtered(
                    gst::ClockTime::from_seconds(5),
                    &[gst::MessageType::Eos, gst::MessageType::Error],
                )
                .ok_or_else(|| "Timed out waiting for export EOS".to_string())?;
            match message.view() {
                gst::MessageView::Eos(_) => Ok(()),
                gst::MessageView::Error(err) => Err(format!(
                    "Export pipeline error: {} ({:?})",
                    err.error(),
                    err.debug()
                )),
                _ => Ok(()),
            }
        }

        pub fn stop(&self) -> Result<(), String> {
            self.pipeline
                .set_state(gst::State::Null)
                .map_err(|e| e.to_string())?;
            Ok(())
        }

        pub fn frame_size(&self) -> usize {
            (self.width as usize)
                .saturating_mul(self.height as usize)
                .saturating_mul(4)
        }

        pub fn fps(&self) -> (i32, i32) {
            (self.fps_num, self.fps_den)
        }

    }

    fn frame_pts_nanos(frame_index: u64, fps_num: i32, fps_den: i32) -> u64 {
        let numerator = u128::from(frame_index) * 1_000_000_000u128 * fps_den.max(1) as u128;
        (numerator / fps_num.max(1) as u128).min(u128::from(u64::MAX)) as u64
    }

    fn make_first_available_element(
        candidates: &[&str],
        purpose: &str,
    ) -> Result<(gst::Element, String), String> {
        let mut last_error = None;
        let mut tried_hardware = false;

        for name in candidates {
            let is_hw = name.contains("nv")
                || name.contains("vaapi")
                || name.contains("qsv")
                || name.contains("d3d11");
            if is_hw {
                tried_hardware = true;
            }

            match gst::ElementFactory::make(name).build() {
                Ok(element) => {
                    if is_hw {
                        log::info!("Using HARDWARE {} element `{}`", purpose, name);
                    } else {
                        if tried_hardware {
                            log::warn!(
                                "FALLBACK: Using SOFTWARE {} element `{}` because hardware acceleration failed or was unavailable",
                                purpose, name
                            );
                        } else {
                            log::info!("Using SOFTWARE {} element `{}`", purpose, name);
                        }
                    }
                    return Ok((element, (*name).to_string()));
                }
                Err(err) => {
                    log::debug!("Skipping {} candidate `{}`: {}", purpose, name, err);
                    last_error = Some(err.to_string());
                }
            }
        }

        Err(format!(
            "No {} element was available (candidates: {:?}){}",
            purpose,
            candidates,
            last_error
                .as_deref()
                .map(|err| format!("; last error: {}", err))
                .unwrap_or_default()
        ))
    }
}

#[cfg(not(feature = "gstreamer-backend"))]
mod imp {
    use super::{ExportSettings, Path};

    pub struct Exporter;

    impl Exporter {
        pub fn new<P: AsRef<Path>>(
            _output_path: P,
            _settings: &ExportSettings,
        ) -> Result<Self, String> {
            Err("Video export is unavailable: this build has no GStreamer backend".into())
        }

        pub fn start(&self) -> Result<(), String> {
            Err("Video export is unavailable: this build has no GStreamer backend".into())
        }

        pub fn push_frame(&self, _bytes: &[u8]) -> Result<(), String> {
            Err("Video export is unavailable: this build has no GStreamer backend".into())
        }

        pub fn end_of_stream(&self) -> Result<(), String> {
            Err("Video export is unavailable: this build has no GStreamer backend".into())
        }

        pub fn stop(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn frame_size(&self) -> usize {
            0
        }

        pub fn fps(&self) -> (i32, i32) {
            (0, 1)
        }
    }
}

pub use imp::Exporter;
#[cfg(all(test, not(feature = "gstreamer-backend")))]
mod unavailable_tests {
    #[test]
    fn disabled_backend_cannot_report_export_success_or_create_file() {
        let path = std::env::temp_dir().join(format!(
            "clipstrike-disabled-export-{}.mp4",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let error = super::Exporter::new(&path, &super::ExportSettings::default())
            .err()
            .expect("disabled backend must fail");
        assert!(error.contains("no GStreamer backend"));
        assert!(!path.exists());
    }
}
#[cfg(all(test, feature = "gstreamer-backend"))]
mod tests {
    use super::Exporter;
    use std::path::PathBuf;

    fn temp_output_path() -> PathBuf {
        let unique = format!(
            "clipstrike-export-smoke-{}-{}.mp4",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn exporter_smoke_test_writes_non_empty_file() {
        let output_path = temp_output_path();
        let _ = std::fs::remove_file(&output_path);

        let settings = super::ExportSettings {
            width: 320,
            height: 240,
            fps_num: 30,
            fps_den: 1,
            ..Default::default()
        };
        let exporter = Exporter::new(&output_path, &settings).expect("failed to create exporter");
        exporter.start().expect("failed to start exporter");

        let frame = vec![0x80; exporter.frame_size()];
        exporter.push_frame(&frame).expect("failed to push frame");
        exporter
            .push_frame(&frame)
            .expect("failed to push second frame");
        exporter.end_of_stream().expect("failed to signal eos");
        exporter.stop().expect("failed to stop exporter");

        let metadata = std::fs::metadata(&output_path).expect("export output file missing");
        assert!(metadata.len() > 0, "export output file should not be empty");
        println!("EXPORT_SMOKE_OUTPUT={}", output_path.display());

        if std::env::var_os("CLIPSTRIKE_KEEP_EXPORT_SMOKE_OUTPUT").is_none() {
            let _ = std::fs::remove_file(output_path);
        }
    }

    #[test]
    fn export_muxer_names_match_output_formats() {
        assert_eq!(
            super::muxer_name_for_format(super::ExportFormat::MP4),
            "mp4mux"
        );
        assert_eq!(
            super::muxer_name_for_format(super::ExportFormat::WebM),
            "webmmux"
        );
        assert_eq!(
            super::muxer_name_for_format(super::ExportFormat::MOV),
            "qtmux"
        );
        assert_eq!(
            super::muxer_name_for_format(super::ExportFormat::MKV),
            "matroskamux"
        );
    }

    #[test]
    fn export_encoder_candidates_prefer_hardware_when_enabled() {
        let settings = super::ExportSettings {
            codec: super::VideoCodec::H264,
            use_hardware_acceleration: true,
            ..Default::default()
        };
        let candidates = super::encoder_candidates_for_settings(&settings);
        assert_eq!(candidates.first().copied(), Some("nvh264enc"));
        assert!(candidates.contains(&"x264enc"));
    }
}
