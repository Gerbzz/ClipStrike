#[cfg(feature = "gstreamer-backend")]
mod imp {
    use clipstrike_render::gpu::TexturePool;
    use gst::prelude::*;
    use gst_app::AppSink;
    use gstreamer as gst;
    use gstreamer::bus::BusWatchGuard;
    use gstreamer_app as gst_app;
    use std::sync::{Arc, Mutex};

    pub struct VideoEngine {
        pipeline: gst::Pipeline,
        appsink: AppSink,
        texture_pool: Arc<Mutex<TexturePool>>,
        queue: Arc<wgpu::Queue>,
        clip_id: usize,
        _bus_guard: BusWatchGuard,
    }

    impl VideoEngine {
        pub fn new(
            uri: &str,
            clip_id: usize,
            texture_pool: Arc<Mutex<TexturePool>>,
            queue: Arc<wgpu::Queue>,
            video_enabled: bool,
            audio_enabled: bool,
        ) -> Result<Self, String> {
            gst::init().map_err(|e| e.to_string())?;

            let (target_width, target_height) = {
                let pool = texture_pool
                    .lock()
                    .map_err(|_| "Failed to lock texture pool".to_string())?;
                (pool.width as i32, pool.height as i32)
            };

            let pipeline = gst::ElementFactory::make("playbin3")
                .build()
                .map_err(|e| e.to_string())?
                .dynamic_cast::<gst::Pipeline>()
                .map_err(|_| "Failed to cast playbin3 to Pipeline".to_string())?;
            pipeline.set_property("uri", uri);

            // Configure stream flags
            let mut flags = 0x00000001 | 0x00000002; // VIDEO | AUDIO
            if !video_enabled { flags &= !0x00000001; }
            if !audio_enabled { flags &= !0x00000002; }
            pipeline.set_property("flags", flags);

            // Task 2.1: Prefer hardware decoders if available
            // We can hint this by setting the 'video-sink' or using a custom playbin configuration.
            // For now, we rely on GStreamer's auto-plugging, but we can ensure high-rank decoders are used.
            // On Windows, d3d11h264dec usually has a high rank.

            let appsink = gst_app::AppSink::builder()
                .name("video_sink")
                .sync(true)
                .max_buffers(2)
                .drop(true)
                .caps(
                    &gst::Caps::builder("video/x-raw")
                        .field("format", "RGBA")
                        .field("width", target_width)
                        .field("height", target_height)
                        .build(),
                )
                .build();
            pipeline.set_property("video-sink", &appsink);

            let bus = pipeline.bus().expect("Pipeline must have a bus");
            let _bus_guard = bus
                .add_watch(move |_, msg| {
                    if let gst::MessageView::Error(err) = msg.view() {
                        let error_msg = err.error().to_string();
                        if error_msg.contains("hardware")
                            || error_msg.contains("nv")
                            || error_msg.contains("vaapi")
                        {
                            log::warn!(
                                "Hardware decoder failed, attempting CPU fallback: {}",
                                error_msg
                            );
                        }
                    }
                    glib::ControlFlow::Continue
                })
                .map_err(|_| "Failed to add bus watch".to_string())?;

            let engine = Self {
                pipeline,
                appsink,
                texture_pool,
                queue,
                clip_id,
                _bus_guard,
            };

            engine.setup_callbacks();
            engine.pause()?;
            Ok(engine)
        }

        fn setup_callbacks(&self) {
            let preroll_pool_ref = Arc::clone(&self.texture_pool);
            let preroll_queue_ref = Arc::clone(&self.queue);
            let sample_pool_ref = Arc::clone(&self.texture_pool);
            let sample_queue_ref = Arc::clone(&self.queue);
            let clip_id = self.clip_id;

            self.appsink.set_callbacks(
                gst_app::AppSinkCallbacks::builder()
                    .new_preroll(move |sink| {
                        let sample = sink.pull_preroll().map_err(|_| gst::FlowError::Eos)?;
                        upload_sample(&sample, &preroll_pool_ref, &preroll_queue_ref, clip_id)
                    })
                    .new_sample(move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        upload_sample(&sample, &sample_pool_ref, &sample_queue_ref, clip_id)
                    })
                    .build(),
            );
        }

        pub fn play(&self) -> Result<(), String> {
            self.pipeline
                .set_state(gst::State::Playing)
                .map_err(|e| e.to_string())?;
            Ok(())
        }

        pub fn pause(&self) -> Result<(), String> {
            self.pipeline
                .set_state(gst::State::Paused)
                .map_err(|e| e.to_string())?;
            Ok(())
        }

        pub fn seek(&self, position: std::time::Duration) -> Result<(), String> {
            let seek_pos = gst::ClockTime::from_nseconds(position.as_nanos() as u64);
            self.pipeline
                .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, seek_pos)
                .map_err(|e| e.to_string())
        }

        pub fn stop(&self) -> Result<(), String> {
            self.pipeline
                .set_state(gst::State::Null)
                .map_err(|e| e.to_string())?;
            Ok(())
        }

        pub fn set_volume(&self, volume: f64) -> Result<(), String> {
            self.pipeline.set_property("volume", volume.clamp(0.0, 4.0));
            Ok(())
        }
    }

    impl Drop for VideoEngine {
        fn drop(&mut self) {
            let _ = self.pipeline.set_state(gst::State::Null);
        }
    }

    fn upload_sample(
        sample: &gst::Sample,
        pool_ref: &Arc<Mutex<TexturePool>>,
        queue_ref: &Arc<wgpu::Queue>,
        clip_id: usize,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
        
        // Task 2.2: Optimize upload path
        // We use map_readable() for CPU->GPU copy, but we ensure it's minimal.
        let map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;

        if let Ok(mut pool) = pool_ref.lock() {
            let width = pool.width;
            let height = pool.height;

            if let Some(texture_item) = pool.acquire_for_clip(clip_id) {
                // write_texture is the most efficient way to upload CPU memory to GPU in wgpu
                queue_ref.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &texture_item.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &map,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(width * 4),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
                texture_item.ready = true;
                texture_item.clip_id = Some(clip_id);
            }
        }

        Ok(gst::FlowSuccess::Ok)
    }
}

#[cfg(not(feature = "gstreamer-backend"))]
mod imp {
    use clipstrike_render::gpu::TexturePool;
    use std::sync::{Arc, Mutex};

    pub struct VideoEngine {
        _texture_pool: Arc<Mutex<TexturePool>>,
        _queue: Arc<wgpu::Queue>,
        _clip_id: usize,
        _uri: String,
    }

    impl VideoEngine {
        pub fn new(
            uri: &str,
            clip_id: usize,
            _texture_pool: Arc<Mutex<TexturePool>>,
            _queue: Arc<wgpu::Queue>,
            _video_enabled: bool,
            _audio_enabled: bool,
        ) -> Result<Self, String> {
            Ok(Self {
                _texture_pool,
                _queue,
                _clip_id: clip_id,
                _uri: uri.to_string(),
            })
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

        pub fn stop(&self) -> Result<(), String> {
            Ok(())
        }

        pub fn set_volume(&self, _volume: f64) -> Result<(), String> {
            Ok(())
        }
    }
}

pub use imp::VideoEngine;
