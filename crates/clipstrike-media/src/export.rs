use std::path::{Path, PathBuf};

#[cfg(feature = "gstreamer-backend")]
mod imp {
    use super::{Path, PathBuf};
    use gst::prelude::*;
    use gstreamer as gst;
    use gstreamer_app as gst_app;
    use gstreamer_video as gst_video;

    /// The Export Pipeline for rendering the project to a file.
    pub struct Exporter {
        pipeline: gst::Pipeline,
        appsrc: gst_app::AppSrc,
        width: u32,
        height: u32,
        fps_num: i32,
        fps_den: i32,
        _output_path: PathBuf,
    }

    impl Exporter {
        pub fn new<P: AsRef<Path>>(
            output_path: P,
            width: u32,
            height: u32,
            fps_num: i32,
            fps_den: i32,
        ) -> Result<Self, String> {
            gst::init().map_err(|e| e.to_string())?;

            let pipeline = gst::Pipeline::with_name("export_pipeline");
            let appsrc = gst::ElementFactory::make("appsrc")
                .build()
                .map_err(|e| e.to_string())?;
            let videoconvert = gst::ElementFactory::make("videoconvert")
                .build()
                .map_err(|e| e.to_string())?;
            let encoder = gst::ElementFactory::make("x264enc")
                .build()
                .map_err(|e| e.to_string())?;
            let muxer = gst::ElementFactory::make("mp4mux")
                .build()
                .map_err(|e| e.to_string())?;
            let filesink = gst::ElementFactory::make("filesink")
                .build()
                .map_err(|e| e.to_string())?;

            filesink.set_property("location", output_path.as_ref().to_str().unwrap());

            let video_info =
                gst_video::VideoInfo::builder(gst_video::VideoFormat::Rgba, width, height)
                    .fps(gst::Fraction::new(fps_num, fps_den))
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

            pipeline
                .add_many(&[
                    appsrc.upcast_ref(),
                    &videoconvert,
                    &encoder,
                    &muxer,
                    &filesink,
                ])
                .map_err(|e| e.to_string())?;

            gst::Element::link_many(&[
                appsrc.upcast_ref(),
                &videoconvert,
                &encoder,
                &muxer,
                &filesink,
            ])
            .map_err(|e| e.to_string())?;

            Ok(Self {
                pipeline,
                appsrc,
                width,
                height,
                fps_num,
                fps_den,
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
            {
                let buffer_mut = buffer
                    .get_mut()
                    .ok_or_else(|| "Failed to get mutable export buffer".to_string())?;
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
            Ok(())
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
}

#[cfg(not(feature = "gstreamer-backend"))]
mod imp {
    use super::{Path, PathBuf};

    pub struct Exporter {
        _output_path: PathBuf,
        width: u32,
        height: u32,
        fps_num: i32,
        fps_den: i32,
    }

    impl Exporter {
        pub fn new<P: AsRef<Path>>(
            output_path: P,
            width: u32,
            height: u32,
            fps_num: i32,
            fps_den: i32,
        ) -> Result<Self, String> {
            Ok(Self {
                _output_path: output_path.as_ref().to_path_buf(),
                width,
                height,
                fps_num,
                fps_den,
            })
        }

        pub fn start(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn push_frame(&self, _bytes: &[u8]) -> Result<(), String> {
            Ok(())
        }

        pub fn end_of_stream(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn stop(&self) -> Result<(), String> {
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
}

pub use imp::Exporter;
