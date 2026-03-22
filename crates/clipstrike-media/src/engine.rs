#[cfg(feature = "gstreamer-backend")]
mod imp {
    use clipstrike_audio::{AudioHandle, decode_f32le_samples};
    use gstreamer as gst;
    use gstreamer_app as gst_app;
    use gstreamer_app::prelude::*;
    use gstreamer_gl as gst_gl;
    #[cfg(target_os = "windows")]
    mod d3d12_ffi {
        use super::*;
        use core::ffi::c_void;

        #[link(name = "gstd3d12-1.0")]
        unsafe extern "C" {
            pub fn gst_is_d3d12_memory(mem: *mut gst::ffi::GstMemory) -> glib::ffi::gboolean;
            pub fn gst_d3d12_memory_get_nt_handle(
                mem: *mut c_void,
                handle: *mut *mut c_void,
            ) -> glib::ffi::gboolean;
        }
    }
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread;

    fn ghost_pad_from_static_pad(
        element: &gst::Element,
        pad_name: &str,
    ) -> Result<gst::GhostPad, String> {
        let Some(pad) = element.static_pad(pad_name) else {
            return Err(format!("{} must have a {} pad", element.name(), pad_name));
        };
        gst::GhostPad::with_target(&pad).map_err(|e| e.to_string())
    }

    fn run_catching_panics<F>(context: &str, setup: F) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = match catch_unwind(AssertUnwindSafe(setup)) {
            Ok(result) => result,
            Err(_) => Err(format!("{context} panicked")),
        };
        std::panic::set_hook(previous_hook);
        result
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(super) enum SeekMode {
        Discrete,
        Interactive,
    }

    pub(super) fn seek_flags(mode: SeekMode) -> gst::SeekFlags {
        let mut flags = gst::SeekFlags::ACCURATE | gst::SeekFlags::FLUSH;
        if matches!(mode, SeekMode::Interactive) {
            flags |= gst::SeekFlags::TRICKMODE;
        }
        flags
    }

    pub struct VideoEngine {
        pipeline: gst::Pipeline,
        appsink: gst_app::AppSink,
        audio_appsink: Option<gst_app::AppSink>,
        audio_handle: Option<AudioHandle>,
        clip_id: usize,
        _frame_width: i32,
        _frame_height: i32,
        frame_sender: Option<tokio::sync::mpsc::Sender<clipstrike_render::gpu::FrameUploadRequest>>,
        bus_stop: Arc<AtomicBool>,
        bus_task: Option<thread::JoinHandle<()>>,
    }

    impl VideoEngine {
        pub fn new(
            uri: &str,
            clip_id: usize,
            frame_width: i32,
            frame_height: i32,
            video_enabled: bool,
            audio_enabled: bool,
            frame_sender: Option<
                tokio::sync::mpsc::Sender<clipstrike_render::gpu::FrameUploadRequest>,
            >,
            event_sender: Option<
                tokio::sync::mpsc::UnboundedSender<crate::engine_manager::EngineEvent>,
            >,
            audio_handle: Option<AudioHandle>,
        ) -> Result<Self, String> {
            gst::init().map_err(|e| e.to_string())?;
            crate::decoder_affinity::prefer_hardware_decoders();
            log::info!(
                "Creating VideoEngine clip_id={} uri={} video_enabled={} audio_enabled={}",
                clip_id,
                uri,
                video_enabled,
                audio_enabled
            );

            let pipeline = gst::ElementFactory::make("playbin3")
                .build()
                .map_err(|e| e.to_string())?
                .dynamic_cast::<gst::Pipeline>()
                .map_err(|_| "Failed to cast playbin3 to Pipeline".to_string())?;
            pipeline.set_property("uri", uri);

            let appsink = gst_app::AppSink::builder()
                .name("video_sink")
                .sync(true)
                .max_buffers(2)
                .drop(true)
                .caps(&video_sink_caps_for_runtime(
                    frame_width,
                    frame_height,
                    video_enabled,
                ))
                .build();

            if video_enabled {
                let hardware_sink_result = run_catching_panics(
                    "hardware preview sink setup",
                    || {
                        // Configure hardware-accelerated preview pipeline for the current platform.
                        let sink_bin = gst::Bin::new();
                        #[cfg(target_os = "windows")]
                        {
                            let d3d12upload = gst::ElementFactory::make("d3d12upload")
                                .build()
                                .map_err(|e| e.to_string())?;
                            let d3d12convert = gst::ElementFactory::make("d3d12convert")
                                .build()
                                .map_err(|e| e.to_string())?;

                            log::debug!(
                                "VideoEngine::new configuring sink clip_id={} uri={} prefer_d3d12_memory=true frame={}x{}",
                                clip_id,
                                uri,
                                frame_width,
                                frame_height
                            );

                            sink_bin
                                .add_many(&[&d3d12upload, &d3d12convert, appsink.upcast_ref()])
                                .map_err(|e| e.to_string())?;
                            gst::Element::link_many(&[
                                &d3d12upload,
                                &d3d12convert,
                                appsink.upcast_ref(),
                            ])
                            .map_err(|e| e.to_string())?;

                            let ghost_pad = ghost_pad_from_static_pad(&d3d12upload, "sink")?;
                            sink_bin.add_pad(&ghost_pad).map_err(|e| e.to_string())?;
                            log::debug!(
                                "Configured GStreamer sink clip_id={} prefer_d3d12_memory=true frame={}x{} sink_pad={}",
                                clip_id,
                                frame_width,
                                frame_height,
                                ghost_pad.name()
                            );

                            pipeline.set_property("video-sink", &sink_bin);
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            // Configure hardware-accelerated pipeline with glupload.
                            let glupload = gst::ElementFactory::make("glupload")
                                .build()
                                .map_err(|e| e.to_string())?;
                            let glcolorconvert = gst::ElementFactory::make("glcolorconvert")
                                .build()
                                .map_err(|e| e.to_string())?;

                            log::debug!(
                                "VideoEngine::new configuring sink clip_id={} uri={} prefer_gl_memory=true frame={}x{}",
                                clip_id,
                                uri,
                                frame_width,
                                frame_height
                            );

                            sink_bin
                                .add_many(&[&glupload, &glcolorconvert, appsink.upcast_ref()])
                                .map_err(|e| e.to_string())?;
                            gst::Element::link_many(&[
                                &glupload,
                                &glcolorconvert,
                                appsink.upcast_ref(),
                            ])
                            .map_err(|e| e.to_string())?;

                            let ghost_pad = ghost_pad_from_static_pad(&glupload, "sink")?;
                            sink_bin.add_pad(&ghost_pad).map_err(|e| e.to_string())?;
                            log::debug!(
                                "Configured GStreamer sink clip_id={} prefer_gl_memory=true frame={}x{} sink_pad={}",
                                clip_id,
                                frame_width,
                                frame_height,
                                ghost_pad.name()
                            );

                            pipeline.set_property("video-sink", &sink_bin);
                        }
                        Ok(())
                    },
                );

                if let Err(err) = hardware_sink_result {
                    log::warn!(
                        "Falling back to direct appsink for clip_id={} uri={} because hardware preview sink setup failed: {}",
                        clip_id,
                        uri,
                        err
                    );
                    let cpu_caps = video_sink_caps(frame_width, frame_height);
                    appsink.set_property("caps", &cpu_caps);
                    pipeline.set_property("video-sink", &appsink);
                }
            } else {
                let video_sink = gst::ElementFactory::make("fakesink")
                    .build()
                    .map_err(|e| e.to_string())?;
                log::debug!(
                    "VideoEngine::new configuring sink clip_id={} uri={} prefer_gl_memory=false frame={}x{}",
                    clip_id,
                    uri,
                    frame_width,
                    frame_height
                );
                pipeline.set_property("video-sink", &video_sink);
            }
            pipeline.set_property("buffer-size", 2i32);

            let bus = pipeline.bus().ok_or_else(|| {
                log::error!(
                    "VideoEngine::new pipeline missing bus for clip_id={} uri={}",
                    clip_id,
                    uri
                );
                "Pipeline must have a bus".to_string()
            })?;
            let bus_stop = Arc::new(AtomicBool::new(false));
            let bus_stop_for_thread = Arc::clone(&bus_stop);
            let event_sender_for_thread = event_sender.clone();
            let bus_task = thread::spawn(move || {
                while !bus_stop_for_thread.load(Ordering::Relaxed) {
                    let Some(msg) = bus.timed_pop(Some(gst::ClockTime::from_mseconds(250))) else {
                        continue;
                    };

                    match msg.view() {
                        gst::MessageView::Error(err) => {
                            log::error!("GStreamer Error: {} ({:?})", err.error(), err.debug());
                        }
                        gst::MessageView::Eos(_) => {
                            log::info!("GStreamer EOS reached for clip_id={}", clip_id);
                            if let Some(sender) = &event_sender_for_thread {
                                let _ =
                                    sender.send(crate::engine_manager::EngineEvent::EndOfStream {
                                        clip_id,
                                    });
                            }
                            break;
                        }
                        _ => {}
                    }
                }
            });

            let mut audio_appsink = None;

            if audio_enabled {
                let sink = gst_app::AppSink::builder()
                    .name("audio_sink")
                    .sync(true)
                    .max_buffers(10)
                    .drop(true)
                    .caps(
                        &gst::Caps::builder("audio/x-raw")
                            .field("format", "F32LE")
                            .field("layout", "interleaved")
                            .field("channels", 2)
                            .field("rate", 44100)
                            .build(),
                    )
                    .build();
                pipeline.set_property("audio-sink", &sink);
                audio_appsink = Some(sink);
            } else {
                let audio_sink = gst::ElementFactory::make("fakesink")
                    .build()
                    .map_err(|e| e.to_string())?;
                pipeline.set_property("audio-sink", &audio_sink);
            }

            let engine = Self {
                pipeline,
                appsink,
                audio_appsink,
                audio_handle,
                clip_id,
                _frame_width: frame_width,
                _frame_height: frame_height,
                frame_sender,
                bus_stop,
                bus_task: Some(bus_task),
            };

            engine.setup_callbacks();
            engine.pause()?;
            Ok(engine)
        }

        fn setup_callbacks(&self) {
            let clip_id = self.clip_id;
            let preroll_frame_sender = self.frame_sender.clone();
            let sample_frame_sender = self.frame_sender.clone();

            self.appsink.set_callbacks(
                gst_app::AppSinkCallbacks::builder()
                    .new_preroll(move |sink| {
                        let sample = sink.pull_preroll().map_err(|_| gst::FlowError::Eos)?;
                        upload_sample(&sample, clip_id, preroll_frame_sender.clone())
                    })
                    .new_sample(move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        upload_sample(&sample, clip_id, sample_frame_sender.clone())
                    })
                    .build(),
            );

            if let (Some(audio_sink), Some(audio_handle)) =
                (&self.audio_appsink, &self.audio_handle)
            {
                let handle_ref = audio_handle.clone();
                let preroll_handle_ref = handle_ref.clone();
                let cid = self.clip_id;
                audio_sink.set_callbacks(
                    gst_app::AppSinkCallbacks::builder()
                        .new_preroll(move |sink| {
                            let sample = sink.pull_preroll().map_err(|_| gst::FlowError::Eos)?;
                            Self::forward_audio_sink_sample(sample, &preroll_handle_ref, cid)
                        })
                        .new_sample(move |sink| {
                            let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                            Self::forward_audio_sink_sample(sample, &handle_ref, cid)
                        })
                        .build(),
                );
            }
        }

        fn forward_audio_sink_sample(
            sample: gst::Sample,
            handle_ref: &AudioHandle,
            cid: usize,
        ) -> Result<gst::FlowSuccess, gst::FlowError> {
            let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
            let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
            match decode_f32le_samples(map.as_ref()) {
                Ok(new_samples) => handle_ref.push_samples(cid, new_samples),
                Err(err) => {
                    log::warn!(
                        "Skipping audio buffer for clip {} because it could not be decoded safely: {}",
                        cid,
                        err
                    );
                }
            }
            Ok(gst::FlowSuccess::Ok)
        }

        pub fn play(&self) -> Result<(), String> {
            self.pipeline
                .set_state(gst::State::Playing)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }

        pub fn pause(&self) -> Result<(), String> {
            self.pipeline
                .set_state(gst::State::Paused)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }

        pub fn seek(&self, position: std::time::Duration) -> Result<(), String> {
            self.seek_with_mode(position, SeekMode::Discrete)
        }

        pub fn seek_interactive(&self, position: std::time::Duration) -> Result<(), String> {
            self.seek_with_mode(position, SeekMode::Interactive)
        }

        fn seek_with_mode(
            &self,
            position: std::time::Duration,
            mode: SeekMode,
        ) -> Result<(), String> {
            self.pipeline
                .seek_simple(
                    seek_flags(mode),
                    gst::ClockTime::from_nseconds(position.as_nanos() as u64),
                )
                .map_err(|e| e.to_string())
        }

        pub fn stop(&self) -> Result<(), String> {
            self.pipeline
                .set_state(gst::State::Null)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }

        pub fn set_volume(&self, volume: f64) -> Result<(), String> {
            self.pipeline.set_property("volume", volume);
            Ok(())
        }

        pub fn get_audio_samples(&self) -> Arc<Vec<f32>> {
            Arc::new(Vec::new())
        }

        /// Query GStreamer's actual playback position in milliseconds.
        /// Returns `None` if the position is not available (e.g. pipeline not ready).
        pub fn query_position_ms(&self) -> Option<i64> {
            self.pipeline
                .query_position::<gst::ClockTime>()
                .map(|t| t.mseconds() as i64)
        }
    }

    impl Drop for VideoEngine {
        fn drop(&mut self) {
            self.bus_stop.store(true, Ordering::Relaxed);
            if let Some(bus_task) = self.bus_task.take() {
                let _ = bus_task.join();
            }
        }
    }

    pub(super) fn video_sink_caps(target_width: i32, target_height: i32) -> gst::Caps {
        video_sink_caps_with_gl_memory(target_width, target_height, false)
    }

    pub(super) fn video_sink_caps_for_runtime(
        target_width: i32,
        target_height: i32,
        prefer_hardware_memory: bool,
    ) -> gst::Caps {
        if !prefer_hardware_memory {
            return video_sink_caps(target_width, target_height);
        }

        #[cfg(target_os = "windows")]
        {
            return video_sink_caps_with_d3d12_memory(target_width, target_height);
        }

        #[cfg(not(target_os = "windows"))]
        {
            return video_sink_caps_with_gl_memory(target_width, target_height, true);
        }
    }

    pub(super) fn video_sink_caps_with_gl_memory(
        target_width: i32,
        target_height: i32,
        prefer_gl_memory: bool,
    ) -> gst::Caps {
        if prefer_gl_memory {
            gst::Caps::builder_full_with_features(gst_gl::CAPS_FEATURES_MEMORY_GL_MEMORY.clone())
                .structure(
                    gst::Structure::builder("video/x-raw")
                        .field("format", "RGBA")
                        .field("width", target_width)
                        .field("height", target_height)
                        .build(),
                )
                .build()
        } else {
            gst::Caps::builder("video/x-raw")
                .field("format", "RGBA")
                .field("width", target_width)
                .field("height", target_height)
                .build()
        }
    }

    pub(super) fn video_sink_caps_with_d3d12_memory(
        target_width: i32,
        target_height: i32,
    ) -> gst::Caps {
        gst::Caps::builder_full_with_features(gst::CapsFeatures::new(["memory:D3D12Memory"]))
            .structure(
                gst::Structure::builder("video/x-raw")
                    .field("format", "RGBA")
                    .field("width", target_width)
                    .field("height", target_height)
                    .build(),
            )
            .build()
    }

    fn map_buffer_pixels(buffer: &gst::BufferRef) -> Result<Vec<u8>, gst::FlowError> {
        match buffer.map_readable() {
            Ok(map) => Ok(map.as_slice().to_vec()),
            Err(_) => {
                let copied_buffer = buffer.copy_deep().map_err(|_| gst::FlowError::Error)?;
                let map = copied_buffer
                    .map_readable()
                    .map_err(|_| gst::FlowError::Error)?;
                Ok(map.as_slice().to_vec())
            }
        }
    }

    fn sample_external_texture_id(sample: &gst::Sample) -> Option<u32> {
        let buffer = sample.buffer()?;
        buffer.iter_memories().find_map(|memory| {
            memory
                .downcast_memory_ref::<gst_gl::GLMemory>()
                .map(|gl_memory| gl_memory.texture_id())
        })
    }

    #[cfg(target_os = "windows")]
    fn sample_native_d3d12_shared_handle(sample: &gst::Sample) -> Option<usize> {
        let buffer = sample.buffer()?;
        buffer.iter_memories().find_map(|memory| unsafe {
            let memory_ptr = memory as *const _ as *mut gst::ffi::GstMemory;
            if d3d12_ffi::gst_is_d3d12_memory(memory_ptr) == 0 {
                return None;
            }

            let mut handle = core::ptr::null_mut();
            if d3d12_ffi::gst_d3d12_memory_get_nt_handle(memory_ptr as *mut _, &mut handle) == 0 {
                return None;
            }
            if handle.is_null() {
                return None;
            }
            Some(handle as usize)
        })
    }

    fn frame_upload_request_from_buffer(
        buffer: &gst::BufferRef,
        clip_id: usize,
        external_texture_id: Option<u32>,
        native_d3d12_shared_handle: Option<usize>,
        timestamp_ms: i64,
    ) -> Result<clipstrike_render::gpu::FrameUploadRequest, gst::FlowError> {
        let pixels = map_buffer_pixels(buffer)?;

        Ok(clipstrike_render::gpu::FrameUploadRequest {
            clip_id,
            timestamp_ms,
            external_texture_id,
            native_d3d12_shared_handle,
            pixels,
        })
    }

    fn frame_upload_request_from_sample(
        sample: &gst::Sample,
        clip_id: usize,
    ) -> Result<clipstrike_render::gpu::FrameUploadRequest, gst::FlowError> {
        let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
        let timestamp = buffer.pts().map(|t| t.mseconds() as i64).unwrap_or(0);
        let external_texture_id = sample_external_texture_id(sample);
        #[cfg(target_os = "windows")]
        let native_d3d12_shared_handle = sample_native_d3d12_shared_handle(sample);
        #[cfg(not(target_os = "windows"))]
        let native_d3d12_shared_handle = None;

        frame_upload_request_from_buffer(
            buffer,
            clip_id,
            external_texture_id,
            native_d3d12_shared_handle,
            timestamp,
        )
    }

    fn upload_sample(
        sample: &gst::Sample,
        clip_id: usize,
        frame_sender: Option<tokio::sync::mpsc::Sender<clipstrike_render::gpu::FrameUploadRequest>>,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let request = frame_upload_request_from_sample(sample, clip_id)?;
        if let Some(sender) = frame_sender {
            let _ = sender.try_send(request);
        }

        Ok(gst::FlowSuccess::Ok)
    }

    #[cfg(all(test, feature = "gstreamer-backend"))]
    mod tests {
        use super::{
            frame_upload_request_from_buffer, frame_upload_request_from_sample,
            ghost_pad_from_static_pad, run_catching_panics, sample_external_texture_id,
            seek_flags, video_sink_caps, video_sink_caps_with_gl_memory, SeekMode,
        };
        use gstreamer as gst;

        #[test]
        fn video_sink_caps_uses_valid_structure_name_for_cpu_rgba_frames() {
            gst::init().expect("gstreamer init");

            let caps = video_sink_caps(1920, 1080);
            let structure = caps.structure(0).expect("caps structure");

            assert_eq!(structure.name(), "video/x-raw");
            assert_eq!(structure.get::<i32>("width").unwrap(), 1920);
            assert_eq!(structure.get::<i32>("height").unwrap(), 1080);
            assert_eq!(structure.get::<&str>("format").unwrap(), "RGBA");
            let features = caps
                .features(0)
                .map(|features| features.to_string())
                .unwrap_or_default();
            assert!(
                features.is_empty() || features == "memory:SystemMemory",
                "unexpected cpu caps features: {features}"
            );
        }

        #[test]
        fn video_sink_caps_uses_glmemory_features_when_preferred() {
            gst::init().expect("gstreamer init");

            let caps = video_sink_caps_with_gl_memory(1920, 1080, true);
            let structure = caps.structure(0).expect("caps structure");

            assert_eq!(structure.name(), "video/x-raw");
            assert_eq!(structure.get::<i32>("width").unwrap(), 1920);
            assert_eq!(structure.get::<i32>("height").unwrap(), 1080);
            assert_eq!(structure.get::<&str>("format").unwrap(), "RGBA");
            assert_eq!(
                caps.features(0)
                    .map(|features| features.to_string())
                    .unwrap_or_default(),
                "memory:GLMemory"
            );
        }


        #[test]
        fn discrete_seek_flushes_and_uses_accurate_positioning() {
            let flags = seek_flags(SeekMode::Discrete);

            assert!(flags.contains(gst::SeekFlags::FLUSH));
            assert!(flags.contains(gst::SeekFlags::ACCURATE));
            assert!(!flags.contains(gst::SeekFlags::KEY_UNIT));
        }

        #[test]
        fn interactive_seek_flushes_and_uses_accurate_trickmode() {
            let flags = seek_flags(SeekMode::Interactive);

            assert!(flags.contains(gst::SeekFlags::FLUSH));
            assert!(flags.contains(gst::SeekFlags::ACCURATE));
            assert!(flags.contains(gst::SeekFlags::TRICKMODE));
        }

        #[test]
        fn video_engine_new_can_configure_playbin3_for_preview_playback() {
            gst::init().expect("gstreamer init");
            let media_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("PythonDatatempclipstrike-gst-launch-mkv.mkv");
            let uri = format!(
                "file:///{}",
                media_path.to_string_lossy().replace('\\', "/")
            );

            let engine =
                super::VideoEngine::new(&uri, 77, 1920, 1080, true, false, None, None, None)
                    .expect("VideoEngine::new should not panic or fail for preview playback");

            drop(engine);
        }

        #[test]
        fn hardware_sink_setup_panics_are_reported_as_errors() {
            let result =
                run_catching_panics("hardware preview sink setup", || -> Result<(), String> {
                    panic!("simulated sink setup panic");
                });

            let err = result.expect_err("panic should be converted into an error");
            assert!(
                err.contains("hardware preview sink setup panicked"),
                "unexpected error: {err}"
            );
        }

        #[test]
        fn sample_external_texture_id_returns_none_for_cpu_buffer() {
            gst::init().expect("gstreamer init");
            let buffer = gst::Buffer::from_mut_slice(vec![1u8, 2, 3, 4]);
            let sample = gst::Sample::builder().buffer(&buffer).build();

            assert_eq!(sample_external_texture_id(&sample), None);
        }

        #[test]
        fn frame_upload_request_from_sample_extracts_pixels() {
            gst::init().expect("gstreamer init");
            let buffer = gst::Buffer::from_mut_slice(vec![1u8, 2, 3, 4]);
            let sample = gst::Sample::builder().buffer(&buffer).build();

            let request = frame_upload_request_from_sample(&sample, 9).expect("frame request");
            assert_eq!(request.clip_id, 9);
            assert_eq!(request.timestamp_ms, 0);
            assert_eq!(request.external_texture_id, None);
            assert_eq!(request.native_d3d12_shared_handle, None);
            assert_eq!(request.pixels, vec![1, 2, 3, 4]);
        }

        #[test]
        fn frame_upload_request_from_buffer_preserves_external_metadata_and_pixels() {
            gst::init().expect("gstreamer init");
            let buffer = gst::Buffer::from_mut_slice(vec![9u8, 8, 7, 6]);

            let request = frame_upload_request_from_buffer(&buffer, 42, Some(77), None, 123)
                .expect("frame request");
            assert_eq!(request.clip_id, 42);
            assert_eq!(request.timestamp_ms, 123);
            assert_eq!(request.external_texture_id, Some(77));
            assert_eq!(request.native_d3d12_shared_handle, None);
            assert_eq!(request.pixels, vec![9, 8, 7, 6]);
        }

        #[test]
        fn ghost_pad_from_static_pad_returns_error_when_pad_is_missing() {
            gst::init().expect("gstreamer init");
            let element = gst::ElementFactory::make("fakesrc")
                .build()
                .expect("fakesrc element");

            let err = ghost_pad_from_static_pad(&element, "sink")
                .expect_err("missing pad should be reported as an error");
            assert!(err.contains("sink pad"), "unexpected error: {}", err);
        }
    }
}

#[cfg(not(feature = "gstreamer-backend"))]
mod imp {
    use clipstrike_audio::AudioHandle;
    use std::sync::Arc;

    pub struct VideoEngine {
        _clip_id: usize,
    }

    impl VideoEngine {
        pub fn new(
            _uri: &str,
            clip_id: usize,
            _frame_width: i32,
            _frame_height: i32,
            _video_enabled: bool,
            _audio_enabled: bool,
            #[cfg(not(target_arch = "wasm32"))] _frame_sender: Option<
                tokio::sync::mpsc::Sender<clipstrike_render::gpu::FrameUploadRequest>,
            >,
            #[cfg(target_arch = "wasm32")] _frame_sender: Option<()>,
            _event_sender: Option<
                tokio::sync::mpsc::UnboundedSender<crate::engine_manager::EngineEvent>,
            >,
            _audio_handle: Option<AudioHandle>,
        ) -> Result<Self, String> {
            Ok(Self { _clip_id: clip_id })
        }

        pub fn play(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn pause(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn seek(&self, _position: std::time::Duration) -> Result<(), String> {
            Ok(())
        }

        pub fn seek_interactive(&self, _position: std::time::Duration) -> Result<(), String> {
            Ok(())
        }

        pub fn stop(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn set_volume(&self, _volume: f64) -> Result<(), String> {
            Ok(())
        }

        pub fn get_audio_samples(&self) -> Arc<Vec<f32>> {
            Arc::new(Vec::new())
        }

        pub fn query_position_ms(&self) -> Option<i64> {
            None
        }
    }
}

pub use imp::VideoEngine;
