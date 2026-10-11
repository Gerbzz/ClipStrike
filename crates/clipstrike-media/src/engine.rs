#[cfg(feature = "gstreamer-backend")]
mod imp {
    use clipstrike_audio::{AudioHandle, decode_f32le_samples};
    use clipstrike_core::media_uri::{media_error, safe_backend_detail, source_label};
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
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    };
    use std::thread;
    static NEXT_SEEK_GENERATION: AtomicU64 = AtomicU64::new(1);

    struct SeekEpoch {
        generation: u64,
        seeking: bool,
    }

    fn startup_error(bus: &gst::Bus, uri: &str, fallback: &str) -> String {
        let message = bus
            .pop_filtered(&[gst::MessageType::Error])
            .or_else(|| {
                bus.timed_pop_filtered(
                    Some(gst::ClockTime::from_mseconds(250)),
                    &[gst::MessageType::Error],
                )
            });
        let detail = message.and_then(|msg| match msg.view() {
            gst::MessageView::Error(error) => Some(error.error().to_string()),
            _ => None,
        });
        media_error(uri, "Open failed", &detail.unwrap_or_else(|| fallback.to_string()))
    }

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
        let result = match catch_unwind(AssertUnwindSafe(setup)) {
            Ok(result) => result,
            Err(_) => Err(format!("{context} panicked")),
        };
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
        seek_epoch: Arc<Mutex<SeekEpoch>>,
        seek_call_lock: Mutex<()>,
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
            let source_context = source_label(uri);
            log::info!(
                "Creating VideoEngine clip_id={} source={} video_enabled={} audio_enabled={}",
                clip_id,
                source_context,
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
                                "VideoEngine::new configuring sink clip_id={} source={} prefer_d3d12_memory=true frame={}x{}",
                                clip_id,
                                source_context,
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
                                "VideoEngine::new configuring sink clip_id={} source={} prefer_gl_memory=true frame={}x{}",
                                clip_id,
                                source_context,
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
                        "Falling back to direct appsink for clip_id={} source={} because hardware preview sink setup failed: {}",
                        clip_id,
                        source_context,
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
                    "VideoEngine::new configuring sink clip_id={} source={} prefer_gl_memory=false frame={}x{}",
                    clip_id,
                    source_context,
                    frame_width,
                    frame_height
                );
                pipeline.set_property("video-sink", &video_sink);
            }
            pipeline.set_property("buffer-size", 2i32);

            let bus = pipeline.bus().ok_or_else(|| {
                log::error!(
                    "VideoEngine::new pipeline missing bus for clip_id={} source={}",
                    clip_id,
                    source_context
                );
                "Pipeline must have a bus".to_string()
            })?;
            let bus_stop = Arc::new(AtomicBool::new(false));

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

            let mut engine = Self {
                pipeline,
                appsink,
                audio_appsink,
                audio_handle,
                clip_id,
                _frame_width: frame_width,
                _frame_height: frame_height,
                frame_sender,
                bus_stop,
                bus_task: None,
                seek_epoch: Arc::new(Mutex::new(SeekEpoch {
                    generation: NEXT_SEEK_GENERATION.fetch_add(1, Ordering::Relaxed),
                    seeking: false,
                })),
                seek_call_lock: Mutex::new(()),
            };

            engine.setup_callbacks();
            engine
                .pause()
                .map_err(|error| startup_error(&bus, uri, &error))?;
            // GStreamer changes to PAUSED asynchronously. The editor seeks as
            // soon as a new engine is returned, so wait for preroll here rather
            // than silently losing that first seek.
            let (transition, state, pending) =
                engine.pipeline.state(gst::ClockTime::from_seconds(5));
            if transition.is_err() || !matches!(state, gst::State::Paused | gst::State::Playing) {
                return Err(startup_error(
                    &bus,
                    uri,
                    &format!("preview did not become ready (state={state:?}, pending={pending:?}, result={transition:?})"),
                ));
            }
            let bus_stop_for_thread = Arc::clone(&engine.bus_stop);
            let event_sender_for_thread = event_sender;
            let bus_source_context = source_context.clone();
            let bus_uri = uri.to_owned();
            engine.bus_task = Some(thread::spawn(move || {
                while !bus_stop_for_thread.load(Ordering::Relaxed) {
                    let Some(msg) = bus.timed_pop(Some(gst::ClockTime::from_mseconds(250))) else {
                        continue;
                    };
                    match msg.view() {
                        gst::MessageView::Error(err) => {
                            log::error!(
                                "GStreamer error for clip {} ({}): {}",
                                clip_id,
                                bus_source_context,
                                safe_backend_detail(&bus_uri, &err.error().to_string())
                            );
                        }
                        gst::MessageView::Eos(_) => {
                            log::info!("GStreamer EOS reached for clip_id={}", clip_id);
                            if let Some(sender) = &event_sender_for_thread {
                                let _ = sender.send(crate::engine_manager::EngineEvent::EndOfStream {
                                    clip_id,
                                });
                            }
                            break;
                        }
                        _ => {}
                    }
                }
            }));
            Ok(engine)
        }

        fn setup_callbacks(&self) {
            let clip_id = self.clip_id;
            let preroll_frame_sender = self.frame_sender.clone();
            let sample_frame_sender = self.frame_sender.clone();
            let preroll_epoch = Arc::clone(&self.seek_epoch);
            let sample_epoch = Arc::clone(&self.seek_epoch);

            self.appsink.set_callbacks(
                gst_app::AppSinkCallbacks::builder()
                    .new_preroll(move |sink| {
                        upload_sample(
                            || sink.pull_preroll().map_err(|_| gst::FlowError::Eos),
                            clip_id,
                            &preroll_epoch,
                            preroll_frame_sender.clone(),
                        )
                    })
                    .new_sample(move |sink| {
                        upload_sample(
                            || sink.pull_sample().map_err(|_| gst::FlowError::Eos),
                            clip_id,
                            &sample_epoch,
                            sample_frame_sender.clone(),
                        )
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
            // Serialize callers, but never hold the callback epoch lock while
            // GStreamer flushes: a flush seek may wait for a streaming callback.
            let _seek_call = self.seek_call_lock.lock().unwrap_or_else(|e| e.into_inner());
            self.seek_epoch.lock().unwrap_or_else(|e| e.into_inner()).seeking = true;
            let result = self.pipeline
                .seek_simple(
                    seek_flags(mode),
                    gst::ClockTime::from_nseconds(position.as_nanos() as u64),
                )
                .map_err(|e| e.to_string());
            let mut epoch = self.seek_epoch.lock().unwrap_or_else(|e| e.into_inner());
            epoch.generation = NEXT_SEEK_GENERATION.fetch_add(1, Ordering::Relaxed);
            // A failed seek has no trustworthy post-seek segment to publish.
            // The next successful seek can reopen callback publication.
            epoch.seeking = result.is_err();
            result
        }

        pub fn frame_generation(&self) -> u64 {
            self.seek_epoch.lock().unwrap_or_else(|e| e.into_inner()).generation
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
            // A running pipeline can keep its streaming threads alive while the
            // bus worker is joined. Stop it before waiting for that worker.
            let _ = self.pipeline.set_state(gst::State::Null);
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
        seek_generation: u64,
    ) -> Result<clipstrike_render::gpu::FrameUploadRequest, gst::FlowError> {
        let pixels = map_buffer_pixels(buffer)?;

        Ok(clipstrike_render::gpu::FrameUploadRequest {
            clip_id,
            timestamp_ms,
            pts_ns: buffer.pts().map(|pts| pts.nseconds()),
            duration_ns: buffer.duration().map(|duration| duration.nseconds()),
            seek_generation,
            external_texture_id,
            native_d3d12_shared_handle,
            pixels,
        })
    }

    fn frame_upload_request_from_sample(
        sample: &gst::Sample,
        clip_id: usize,
        seek_generation: u64,
    ) -> Result<clipstrike_render::gpu::FrameUploadRequest, gst::FlowError> {
        let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
        // qtdemux can shift a stream with negative initial DTS forward. The
        // accepted HLAE H.264 capture starts at buffer PTS 33.333 ms while its
        // TIME segment starts there and assigns that frame stream time zero.
        // Export and clip source positions use stream time, so keep the segment
        // conversion with each sample rather than treating raw buffer PTS as
        // the media's source timestamp.
        let pts = buffer.pts().ok_or_else(|| {
            log::warn!("Decoded frame for clip {clip_id} has no presentation timestamp");
            gst::FlowError::NotNegotiated
        })?;
        let time_segment = sample.segment()
            .and_then(|segment| segment.downcast_ref::<gst::ClockTime>())
            .ok_or_else(|| {
                log::warn!("Decoded frame for clip {clip_id} has no TIME segment");
                gst::FlowError::NotNegotiated
            })?;
        // Duration is still expressed in buffer time. Other rates require a
        // matching duration transform before source-interval coverage is safe.
        if time_segment.rate() != 1.0 || time_segment.applied_rate() != 1.0 {
            log::warn!("Decoded frame for clip {clip_id} has unsupported segment rate");
            return Err(gst::FlowError::NotNegotiated);
        }
        // Stream time is the authored source timeline. Segment base is running
        // time for scheduling and must not be substituted here.
        let stream_pts = time_segment.to_stream_time(pts).ok_or_else(|| {
            log::warn!("Decoded frame for clip {clip_id} falls outside its TIME segment");
            gst::FlowError::NotNegotiated
        })?;
        let timestamp = stream_pts.mseconds() as i64;
        let external_texture_id = sample_external_texture_id(sample);
        #[cfg(target_os = "windows")]
        let native_d3d12_shared_handle = sample_native_d3d12_shared_handle(sample);
        #[cfg(not(target_os = "windows"))]
        let native_d3d12_shared_handle = None;

        let mut request = frame_upload_request_from_buffer(
            buffer,
            clip_id,
            external_texture_id,
            native_d3d12_shared_handle,
            timestamp,
            seek_generation,
        )?;
        request.pts_ns = Some(stream_pts.nseconds());
        if request.duration_ns.is_none() {
            request.duration_ns = sample.caps()
                .and_then(|caps| caps.structure(0))
                .and_then(|structure| structure.get::<gst::Fraction>("framerate").ok())
                .and_then(|fps| {
                    (fps.numer() > 0 && fps.denom() > 0).then(||
                        (1_000_000_000u128 * fps.denom() as u128 / fps.numer() as u128) as u64
                    )
                });
        }
        Ok(request)
    }

    fn upload_sample<F>(
        pull: F,
        clip_id: usize,
        seek_epoch: &Mutex<SeekEpoch>,
        frame_sender: Option<tokio::sync::mpsc::Sender<clipstrike_render::gpu::FrameUploadRequest>>,
    ) -> Result<gst::FlowSuccess, gst::FlowError>
    where
        F: FnOnce() -> Result<gst::Sample, gst::FlowError>,
    {
        let initial = seek_epoch.lock().unwrap_or_else(|e| e.into_inner());
        let generation = initial.generation;
        let started_during_seek = initial.seeking;
        drop(initial);
        let sample = pull()?;
        if started_during_seek {
            return Ok(gst::FlowSuccess::Ok);
        }
        let request = frame_upload_request_from_sample(&sample, clip_id, generation)?;
        // Publication and the seek-start marker use the same lock. A callback
        // spanning a seek cannot tag its pre-seek sample with the new epoch.
        let current = seek_epoch.lock().unwrap_or_else(|e| e.into_inner());
        if current.seeking || current.generation != generation {
            return Ok(gst::FlowSuccess::Ok);
        }
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
            seek_flags, upload_sample, video_sink_caps, video_sink_caps_with_gl_memory, SeekEpoch,
            SeekMode,
        };
        use gstreamer as gst;

        #[test]
        fn callback_crossing_seek_cannot_publish_a_pre_seek_sample_as_current() {
            use std::sync::{Arc, Barrier, Mutex};

            gst::init().expect("gstreamer init");
            let epoch = Arc::new(Mutex::new(SeekEpoch { generation: 101, seeking: false }));
            let barrier = Arc::new(Barrier::new(2));
            let epoch_for_seek = Arc::clone(&epoch);
            let barrier_for_seek = Arc::clone(&barrier);
            let seek = std::thread::spawn(move || {
                barrier_for_seek.wait();
                let mut state = epoch_for_seek.lock().unwrap();
                state.seeking = true;
                state.generation = 102;
                state.seeking = false;
            });
            let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
            let mut buffer = gst::Buffer::from_mut_slice(vec![1_u8, 2, 3, 4]);
            buffer.get_mut().unwrap().set_pts(gst::ClockTime::from_mseconds(0));
            let segment = gst::FormattedSegment::<gst::ClockTime>::new();
            let sample = gst::Sample::builder().buffer(&buffer).segment(&segment).build();
            upload_sample(
                || {
                    barrier.wait();
                    seek.join().unwrap();
                    Ok(sample)
                },
                7,
                &epoch,
                Some(sender.clone()),
            ).unwrap();
            assert!(receiver.try_recv().is_err(), "the pre-seek sample must be dropped");

            epoch.lock().unwrap().seeking = true;
            let during_seek = gst::Sample::builder().buffer(&buffer).segment(&segment).build();
            upload_sample(|| Ok(during_seek), 7, &epoch, Some(sender.clone())).unwrap();
            assert!(receiver.try_recv().is_err(), "a callback begun during flush must be dropped");

            epoch.lock().unwrap().seeking = false;
            let after_seek = gst::Sample::builder().buffer(&buffer).segment(&segment).build();
            upload_sample(|| Ok(after_seek), 7, &epoch, Some(sender)).unwrap();
            assert_eq!(receiver.try_recv().unwrap().seek_generation, 102);
        }

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
            let directory = tempfile::tempdir().expect("temporary media directory");
            let media_path = directory.path().join("preview-with-audio.mp4");
            std::fs::write(
                &media_path,
                include_bytes!("../tests/fixtures/preview-with-audio.mp4"),
            )
            .expect("write portable preview fixture");
            let uri = url::Url::from_file_path(&media_path)
                .expect("portable fixture path should have a file URI")
                .to_string();

            let (frame_sender, mut frame_receiver) = tokio::sync::mpsc::channel(16);
            let engine =
                super::VideoEngine::new(&uri, 77, 1920, 1080, true, false, Some(frame_sender), None, None)
                    .expect("VideoEngine::new should not panic or fail for preview playback");
            let startup_generation = engine.frame_generation();
            engine
                .seek(std::time::Duration::from_millis(500))
                .expect("first seek must work before playback starts");
            let seek_generation = engine.frame_generation();
            assert_ne!(startup_generation, seek_generation);
            // A paused seek must still produce a decoded frame. Export and
            // scrubbing cannot depend on calling play() to wake the sink.
            let mut saw_paused_preroll = false;
            for _ in 0..30 {
                while let Ok(frame) = frame_receiver.try_recv() {
                    if frame.seek_generation == seek_generation && frame.timestamp_ms >= 450 {
                        saw_paused_preroll = true;
                        break;
                    }
                }
                if saw_paused_preroll { break; }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(saw_paused_preroll, "paused seek must publish current-generation preroll");
            let mut reached_seek = false;
            for _ in 0..40 {
                if engine.query_position_ms().is_some_and(|position| (450..=600).contains(&position)) {
                    reached_seek = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            assert!(reached_seek, "first seek must reach the requested source region");
            engine.play().expect("play portable fixture");
            let mut saw_new_generation_frame = false;
            for _ in 0..40 {
                while let Ok(frame) = frame_receiver.try_recv() {
                    if frame.seek_generation == seek_generation && frame.timestamp_ms >= 450 {
                        saw_new_generation_frame = true;
                        break;
                    }
                }
                if saw_new_generation_frame { break; }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            assert!(saw_new_generation_frame, "decoded playback must carry the seek generation and source timestamp");
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
        fn caught_setup_panic_still_calls_the_installed_hook() {
            use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

            let calls = Arc::new(AtomicUsize::new(0));
            let calls_for_hook = Arc::clone(&calls);
            let previous_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |_| {
                calls_for_hook.fetch_add(1, Ordering::SeqCst);
            }));
            let result = run_catching_panics("concurrent setup", || -> Result<(), String> {
                panic!("simulated setup failure");
            });
            std::panic::set_hook(previous_hook);

            assert!(result.unwrap_err().contains("concurrent setup panicked"));
            assert!(calls.load(Ordering::SeqCst) >= 1, "engine setup must not suppress the process panic hook");
        }

        #[test]
        fn missing_local_media_error_names_the_file_and_reports_pipeline_failure() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("missing-preview.mp4");
            let uri = url::Url::from_file_path(&path).unwrap().to_string();
            let error = match super::VideoEngine::new(&uri, 91, 320, 180, true, false, None, None, None) {
                Ok(_) => panic!("a missing media file must fail"),
                Err(error) => error,
            };
            assert!(error.contains("missing-preview.mp4"), "{error}");
            assert!(error.starts_with("Open failed for local media"), "{error}");
            assert!(!error.ends_with("Element failed to change its state"), "{error}");
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
            let mut buffer = gst::Buffer::from_mut_slice(vec![1u8, 2, 3, 4]);
            buffer.get_mut().unwrap().set_pts(gst::ClockTime::from_mseconds(0));
            let segment = gst::FormattedSegment::<gst::ClockTime>::new();
            let sample = gst::Sample::builder().buffer(&buffer).segment(&segment).build();

            let request = frame_upload_request_from_sample(&sample, 9, 4).expect("frame request");
            assert_eq!(request.clip_id, 9);
            assert_eq!(request.seek_generation, 4);
            assert_eq!(request.timestamp_ms, 0);
            assert_eq!(request.external_texture_id, None);
            assert_eq!(request.native_d3d12_shared_handle, None);
            assert_eq!(request.pixels, vec![1, 2, 3, 4]);
        }

        #[test]
        fn frame_without_buffer_duration_uses_negotiated_constant_framerate() {
            gst::init().expect("gstreamer init");
            let mut buffer = gst::Buffer::from_mut_slice(vec![1u8, 2, 3, 4]);
            buffer.get_mut().unwrap().set_pts(gst::ClockTime::from_mseconds(100));
            let caps = gst::Caps::builder("video/x-raw")
                .field("framerate", gst::Fraction::new(30, 1)).build();
            let segment = gst::FormattedSegment::<gst::ClockTime>::new();
            let sample = gst::Sample::builder().buffer(&buffer).caps(&caps).segment(&segment).build();

            let request = frame_upload_request_from_sample(&sample, 9, 4).unwrap();
            assert_eq!(request.pts_ns, Some(100_000_000));
            assert_eq!(request.duration_ns, Some(33_333_333));
        }

        #[test]
        fn sample_segment_restores_source_time_after_negative_dts_shift() {
            gst::init().expect("gstreamer init");
            let mut segment = gst::FormattedSegment::<gst::ClockTime>::new();
            segment.set_start(gst::ClockTime::from_nseconds(33_333_333));
            segment.set_time(gst::ClockTime::from_nseconds(0));
            let mut buffer = gst::Buffer::from_mut_slice(vec![1u8, 2, 3, 4]);
            buffer.get_mut().unwrap().set_pts(gst::ClockTime::from_nseconds(33_333_333));
            buffer.get_mut().unwrap().set_duration(gst::ClockTime::from_nseconds(16_666_666));
            let sample = gst::Sample::builder().buffer(&buffer).segment(&segment).build();

            let request = frame_upload_request_from_sample(&sample, 1, 7).unwrap();
            assert_eq!(request.pts_ns, Some(0));
            assert_eq!(request.duration_ns, Some(16_666_666));
            assert_eq!(request.timestamp_ms, 0);
            assert_eq!(request.seek_generation, 7);

            let mut seek_segment = gst::FormattedSegment::<gst::ClockTime>::new();
            seek_segment.set_start(gst::ClockTime::from_nseconds(133_333_333));
            seek_segment.set_time(gst::ClockTime::from_nseconds(100_000_000));
            let mut seek_buffer = gst::Buffer::from_mut_slice(vec![1u8, 2, 3, 4]);
            seek_buffer.get_mut().unwrap().set_pts(gst::ClockTime::from_nseconds(133_333_333));
            let seek_sample = gst::Sample::builder().buffer(&seek_buffer).segment(&seek_segment).build();
            let request = frame_upload_request_from_sample(&seek_sample, 1, 8).unwrap();
            assert_eq!(request.pts_ns, Some(100_000_000));
            assert_eq!(request.timestamp_ms, 100);
            assert_eq!(request.seek_generation, 8);
        }

        #[test]
        fn sample_without_pts_or_time_segment_cannot_be_marked_fresh() {
            gst::init().expect("gstreamer init");
            let buffer = gst::Buffer::from_mut_slice(vec![1u8, 2, 3, 4]);
            let segment = gst::FormattedSegment::<gst::ClockTime>::new();
            let no_pts = gst::Sample::builder().buffer(&buffer).segment(&segment).build();
            assert!(matches!(
                frame_upload_request_from_sample(&no_pts, 1, 7),
                Err(gst::FlowError::NotNegotiated)
            ));

            let mut timestamped_buffer = gst::Buffer::from_mut_slice(vec![1u8, 2, 3, 4]);
            timestamped_buffer.get_mut().unwrap().set_pts(gst::ClockTime::from_mseconds(0));
            // GstSample supplies a default identity TIME segment when none is
            // provided. An explicit non-TIME segment must still be rejected.
            let bytes_segment = gst::FormattedSegment::<gst::format::Bytes>::new();
            let no_segment = gst::Sample::builder()
                .buffer(&timestamped_buffer)
                .segment(&bytes_segment)
                .build();
            assert!(matches!(
                frame_upload_request_from_sample(&no_segment, 1, 7),
                Err(gst::FlowError::NotNegotiated)
            ));
        }

        #[test]
        fn non_unit_segment_rate_cannot_reuse_unscaled_frame_duration() {
            gst::init().expect("gstreamer init");
            let mut buffer = gst::Buffer::from_mut_slice(vec![1u8, 2, 3, 4]);
            buffer.get_mut().unwrap().set_pts(gst::ClockTime::from_mseconds(100));
            buffer.get_mut().unwrap().set_duration(gst::ClockTime::from_mseconds(40));
            let mut segment = gst::FormattedSegment::<gst::ClockTime>::new();
            segment.set_rate(2.0);
            let sample = gst::Sample::builder().buffer(&buffer).segment(&segment).build();
            assert!(matches!(
                frame_upload_request_from_sample(&sample, 1, 7),
                Err(gst::FlowError::NotNegotiated)
            ));
        }

        #[test]
        fn frame_upload_request_from_buffer_preserves_external_metadata_and_pixels() {
            gst::init().expect("gstreamer init");
            let buffer = gst::Buffer::from_mut_slice(vec![9u8, 8, 7, 6]);

            let request = frame_upload_request_from_buffer(&buffer, 42, Some(77), None, 123, 5)
                .expect("frame request");
            assert_eq!(request.clip_id, 42);
            assert_eq!(request.timestamp_ms, 123);
            assert_eq!(request.seek_generation, 5);
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
            _clip_id: usize,
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
            Err("Media playback is unavailable: this build has no GStreamer backend".into())
        }

        pub fn play(&self) -> Result<(), String> {
            Err("Media playback is unavailable: this build has no GStreamer backend".into())
        }

        pub fn pause(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn seek(&self, _position: std::time::Duration) -> Result<(), String> {
            Err("Media playback is unavailable: this build has no GStreamer backend".into())
        }

        pub fn seek_interactive(&self, _position: std::time::Duration) -> Result<(), String> {
            Err("Media playback is unavailable: this build has no GStreamer backend".into())
        }

        pub fn frame_generation(&self) -> u64 {
            0
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

#[cfg(all(test, not(feature = "gstreamer-backend")))]
mod unavailable_tests {
    #[test]
    fn disabled_backend_cannot_report_playback_ready() {
        let error = super::VideoEngine::new(
            "file:///fixture.mp4", 1, 320, 180, true, true, None, None, None,
        )
        .err()
        .expect("disabled backend must fail");
        assert!(error.contains("no GStreamer backend"));
    }
}
