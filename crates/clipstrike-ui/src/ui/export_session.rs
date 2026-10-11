use super::*;

impl ClipStrikeApp {
    pub(super) fn export_failure_message(error: &str) -> String {
        if Self::is_hardware_session_error(error) {
            format!(
                "NVIDIA hardware H.264 encoding could not start. Turn off Hardware H.264 encoding in this dialog and retry with software encoding. Details: {error}"
            )
        } else {
            error.to_owned()
        }
    }

    pub(super) fn is_hardware_session_error(error: &str) -> bool {
        error.contains("gst_nv_encoder_init_session")
            || (error.contains("GstNvH264Enc") && error.contains("Failed to open session"))
    }

    pub(super) fn capture_composited_preview_rgba_buffer(&mut self) -> Result<(u32, u32), String> {
        self.frame_renderer
            .as_ref()
            .ok_or_else(|| "WGPU state missing".to_string())?
            .read_rgba_into(&mut self.save_frame_buffer)
    }

    pub(super) fn apply_pending_frame_uploads(&mut self) {
        let pending = self.collect_pending_frame_upload_requests();
        let Some(frame_renderer) = &mut self.frame_renderer else {
            return;
        };
        for request in pending {
            let clip_id = request.clip_id;
            match frame_renderer.upload(request) {
                Ok(_) => {
                    if let (Some(wgpu_state), Some(frame)) = (
                        &self.wgpu_render_state,
                        frame_renderer.texture_pool.frame_for_clip(clip_id),
                    ) {
                        if let Some(egui_id) = frame.egui_id {
                            wgpu_state
                                .renderer
                                .write()
                                .update_egui_texture_from_wgpu_texture(
                                    &frame_renderer.device,
                                    frame.preview_view(),
                                    wgpu::FilterMode::Linear,
                                    egui_id,
                                );
                        }
                    }
                }
                Err(err) => log::warn!(
                    "Failed to apply frame upload request for clip {}: {}",
                    clip_id,
                    err
                ),
            }
        }
    }

    pub(super) fn render_composited_preview(&self) -> bool {
        let Some(renderer) = &self.frame_renderer else {
            return false;
        };
        let width = renderer.texture_pool.width;
        let height = renderer.texture_pool.height;
        let layers = self
            .compositing_clips_in_render_order()
            .filter(|clip| clip.video_enabled)
            .map(|clip| RenderLayer {
                clip,
                source_ms: (!self.clip_has_static_visual(clip))
                    .then(|| clip.source_time_at(self.playhead_ms)),
                transform: Self::preview_transform_for_clip(clip, self.playhead_ms, width, height),
            });
        renderer.render_at(
            layers,
            self.playhead_ms,
            (self.frame_duration_ms() * 3).max(100),
            self.active_export.is_some(),
        )
    }

    pub(super) fn resize_rgba_buffer(
        pixels: &[u8],
        source_width: u32,
        source_height: u32,
        target_width: u32,
        target_height: u32,
    ) -> Result<Vec<u8>, String> {
        if source_width == target_width && source_height == target_height {
            return Ok(pixels.to_vec());
        }

        let source_image = image::RgbaImage::from_raw(source_width, source_height, pixels.to_vec())
            .ok_or_else(|| "Failed to build export image buffer".to_string())?;
        Ok(image::imageops::resize(
            &source_image,
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        )
        .into_raw())
    }

    pub(super) fn export_settings_for_selected_preset(&self) -> ExportSettings {
        let mut settings = self.app_config
            .export_presets
            .get(self.export_preset_index)
            .map(|preset| preset.settings.clone())
            .unwrap_or_default();
        if let Some(use_hardware) = self.export_hardware_override {
            settings.use_hardware_acceleration = use_hardware;
        }
        settings
    }

    pub(super) fn export_destination_directory(&self) -> Option<PathBuf> {
        self.export_output_path
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| self.app_config.last_export_directory.clone())
    }

    pub(super) fn remember_export_directory(&mut self, path: &Path) {
        if let Some(parent) = path.parent() {
            self.app_config.last_export_directory = Some(parent.to_path_buf());
            self.persist_app_config();
        }
    }

    pub(super) fn begin_export(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.active_export.is_some() {
            return Err("An export is already running".to_string());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.pending_export_preparation.is_some() {
            return Err("An export is already preparing".to_string());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.pending_export_finalization.is_some() {
            return Err("An export is already finalizing".to_string());
        }

        self.published_export_path = None;

        let settings = self.export_settings_for_selected_preset();
        let output_path = if let Some(path) = self.export_output_path.clone() {
            path
        } else {
            self.pick_export_save_path(&settings)
                .ok_or_else(|| "Export cancelled before selecting an output file".to_string())?
        };
        self.export_output_path = Some(output_path.clone());
        if output_path.exists() {
            return Err(format!(
                "Export destination already exists: {}",
                output_path.display()
            ));
        }
        if self.timeline.clips.is_empty() {
            return Err("Add at least one clip to the timeline before exporting".into());
        }

        if settings.width == 0
            || settings.height == 0
            || settings.fps_num <= 0
            || settings.fps_den <= 0
        {
            return Err("Invalid export dimensions or FPS".into());
        }
        let total_frames = export_frame_count_for_duration_ms(
            self.timeline_end_ms(),
            settings.fps_num,
            settings.fps_den,
        );
        if !cfg!(feature = "gstreamer-backend") {
            return Err("Video export is unavailable: this build has no GStreamer backend".into());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let project_snapshot = self.project.clone();
            let clips_snapshot = self.timeline.clips.clone();
            let worker_project = project_snapshot.clone();
            let worker_clips = clips_snapshot.clone();
            let worker_settings = settings.clone();
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let worker_cancel = Arc::clone(&cancel);
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let result = crate::audio_export::prepare_mix_cancellable(
                    &worker_project,
                    &worker_clips,
                    &worker_settings,
                    total_frames,
                    &worker_cancel,
                );
                let _ = sender.send(result);
            });
            self.pending_export_preparation = Some(PendingExportPreparation {
                output_path,
                settings,
                total_frames,
                project_snapshot,
                clips_snapshot,
                receiver,
                cancel,
            });
            self.export_notification = Some("Preparing export media…".into());
            self.show_export_dialog = true;
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return Ok(());
        }
        #[cfg(target_arch = "wasm32")]
        let audio_mix: Option<OfflineMix> = {
            if self.timeline.clips.iter().any(|clip| {
                clip.audio_enabled
                    && self.project.assets.iter().any(|asset| {
                        asset.id == clip.asset_id && Self::media_asset_has_audio(asset)
                    })
            }) {
                return Err("Soundtrack export is unavailable in this web build".into());
            }
            None
        };
        #[cfg(target_arch = "wasm32")]
        self.start_export_with_mix(ctx, output_path, settings, total_frames, audio_mix)
    }

    pub(super) fn start_export_with_mix(
        &mut self,
        ctx: &egui::Context,
        output_path: PathBuf,
        settings: ExportSettings,
        total_frames: u64,
        audio_mix: Option<OfflineMix>,
    ) -> Result<(), String> {
        let file_name = output_path
            .file_name()
            .ok_or_else(|| "Export destination needs a file name".to_string())?;
        let staging_path = output_path.with_file_name(format!(
            ".{}.clipstrike-{}-{}.partial",
            file_name.to_string_lossy(),
            std::process::id(),
            rand::random::<u64>(),
        ));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging_path)
            .map_err(|error| format!("Could not reserve export staging file: {error}"))?;
        let exporter = match Exporter::new(&staging_path, &settings) {
            Ok(exporter) => exporter,
            Err(error) => {
                let _ = std::fs::remove_file(&staging_path);
                return Err(error);
            }
        };
        if let Err(error) = exporter.start() {
            let _ = exporter.stop();
            let _ = std::fs::remove_file(&staging_path);
            return Err(error);
        }
        self.remember_export_directory(&output_path);

        let restore_playhead_ms = self.playhead_ms;
        let restore_is_playing = self.is_playing;
        self.set_playing_state(false);
        self.playhead_ms = 0;
        self.seek_playhead();

        self.active_export = Some(ActiveExportJob {
            output_path,
            staging_path,
            project_snapshot: self.snapshot_project(),
            audio_mix,
            exporter,
            progress: ExportProgress::new(total_frames),
            settings,
            next_frame_ms: 0,
            next_frame_index: 0,
            pending_capture: false,
            capture_ready: false,
            frame_wait_started_at: Instant::now(),
            started_at: Instant::now(),
            restore_playhead_ms,
            restore_is_playing,
        });
        self.export_notification = Some("Export started".to_string());
        self.show_export_dialog = false;
        ctx.request_repaint();
        Ok(())
    }

    pub(super) fn request_active_export_cancel(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(pending) = self.pending_export_preparation.take() {
            pending
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            self.export_notification = Some("Export canceled during media preparation".into());
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(pending) = &self.pending_export_finalization {
            pending
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            self.export_notification = Some("Export cancel requested".into());
            return;
        }
        if let Some(job) = &mut self.active_export {
            job.progress.request_cancel();
            self.export_notification = Some("Export cancel requested".to_string());
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn process_export_preparation(&mut self, ctx: &egui::Context) {
        let result = match self.pending_export_preparation.as_ref() {
            Some(pending) => pending.receiver.try_recv(),
            None => return,
        };
        match result {
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return;
            }
            _ => {}
        }
        let pending = self.pending_export_preparation.take().unwrap();
        let result = match result {
            Ok(mix) => mix,
            Err(_) => Err("Media preparation worker stopped unexpectedly".into()),
        };
        if pending.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            self.export_notification = Some("Export canceled".into());
        } else if self.project != pending.project_snapshot
            || self.timeline.clips != pending.clips_snapshot
        {
            self.export_notification =
                Some("Export canceled: the project changed during media preparation".into());
        } else {
            match result.and_then(|mix| {
                self.start_export_with_mix(
                    ctx,
                    pending.output_path,
                    pending.settings,
                    pending.total_frames,
                    mix,
                )
            }) {
                Ok(()) => {}
                Err(error) => self.export_notification = Some(format!("Export failed: {error}")),
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn process_export_finalization(&mut self, ctx: &egui::Context) {
        if let Some(pending) = &self.pending_export_finalization {
            if !self.export_snapshot_matches(&pending.project_snapshot) {
                pending
                    .cancel
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        let result = match self.pending_export_finalization.as_ref() {
            Some(pending) => pending.receiver.try_recv(),
            None => return,
        };
        if matches!(result, Err(std::sync::mpsc::TryRecvError::Empty)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }
        let pending = self.pending_export_finalization.take().unwrap();
        self.export_notification = Some(match result {
            Ok(Ok(prepared)) => match prepared.publish_if(&pending.output_path, || {
                if pending.cancel.load(std::sync::atomic::Ordering::Relaxed)
                    || !self.export_snapshot_matches(&pending.project_snapshot) {
                    Err("Project changed or export was canceled during finalization; staged output was discarded".into())
                } else {
                    Ok(())
                }
            }) {
                Ok(()) => {
                    self.published_export_path = Some(pending.output_path.clone());
                    format!("Export complete: {}", pending.output_path.display())
                }
                Err(error) => format!("Export failed: {error}"),
            },
            Ok(Err(error)) => format!("Export failed: {error}"),
            Err(_) => "Export failed: finalization worker stopped unexpectedly".into(),
        });
    }

    pub(super) fn restore_after_export(
        &mut self,
        restore_playhead_ms: i64,
        restore_is_playing: bool,
    ) {
        self.playhead_ms = restore_playhead_ms;
        self.seek_playhead();
        self.set_playing_state(restore_is_playing);
    }

    pub(super) fn finalize_export_job(
        &mut self,
        ctx: &egui::Context,
        job: ActiveExportJob,
        outcome: Result<(), String>,
    ) {
        let stop_result = job.exporter.stop();
        let restore_playhead_ms = job.restore_playhead_ms;
        let restore_is_playing = job.restore_is_playing;
        let project_unchanged = self.export_snapshot_matches(&job.project_snapshot);
        let outcome = outcome.and(stop_result).and_then(|_| {
            if project_unchanged {
                Ok(())
            } else {
                Err("Project changed during export; the partial render was discarded".into())
            }
        });
        self.active_export = None;
        if project_unchanged {
            self.restore_after_export(restore_playhead_ms, restore_is_playing);
        }
        if let Err(error) = outcome {
            let _ = std::fs::remove_file(&job.staging_path);
            self.export_notification = Some(format!(
                "Export failed: {}",
                Self::export_failure_message(&error)
            ));
            self.show_export_dialog = Self::is_hardware_session_error(&error);
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let output_path = job.output_path;
            let worker_output = output_path.clone();
            let staging_path = job.staging_path;
            let settings = job.settings;
            let mix = job.audio_mix;
            let frames = job.next_frame_index;
            let project_snapshot = job.project_snapshot;
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let worker_cancel = Arc::clone(&cancel);
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let result = crate::audio_export::prepare_final_export(
                    &staging_path,
                    &worker_output,
                    &settings,
                    mix.as_ref(),
                    frames,
                    &worker_cancel,
                );
                let _ = sender.send(result);
            });
            self.pending_export_finalization = Some(PendingExportFinalization {
                output_path,
                project_snapshot,
                receiver,
                cancel,
            });
            self.export_notification = Some("Verifying encoded output…".into());
            self.show_export_dialog = true;
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let result = if job.audio_mix.is_some() {
                Err("Soundtrack export is unavailable in this web build".to_string())
            } else {
                std::fs::rename(&job.staging_path, &job.output_path)
                    .map_err(|error| format!("Could not publish export: {error}"))
            };
            if result.is_err() {
                let _ = std::fs::remove_file(&job.staging_path);
            }
            self.export_notification = Some(match result {
                Ok(()) => format!("Export complete: {}", job.output_path.display()),
                Err(error) => format!("Export failed: {error}"),
            });
            self.show_export_dialog = false;
        }
    }

    pub(super) fn process_active_export_seek(&mut self, ctx: &egui::Context) {
        if self
            .active_export
            .as_ref()
            .is_some_and(|job| !self.export_snapshot_matches(&job.project_snapshot))
        {
            let job = self.active_export.take().unwrap();
            self.finalize_export_job(
                ctx,
                job,
                Err("Project changed during export; the partial render was discarded".into()),
            );
            return;
        }
        let timeline_end_ms = self.timeline_end_ms();
        let mut next_seek_ms = None;
        let mut cancel_requested = false;
        if let Some(job) = &mut self.active_export {
            cancel_requested = job.progress.cancel_requested();
            if !job.pending_capture {
                next_seek_ms = Some(job.next_frame_ms.min(timeline_end_ms));
                job.pending_capture = true;
                job.capture_ready = false;
                job.frame_wait_started_at = Instant::now();
            }
        }

        if cancel_requested {
            let job = self.active_export.take().expect("export job should exist");
            self.finalize_export_job(ctx, job, Err("Export canceled".to_string()));
            return;
        }

        if let Some(next_seek_ms) = next_seek_ms {
            self.playhead_ms = next_seek_ms;
            self.seek_playhead();
            ctx.request_repaint();
        }
    }

    pub(super) fn process_active_export_capture(&mut self, ctx: &egui::Context) {
        let Some(mut job) = self.active_export.take() else {
            return;
        };

        if !self.export_snapshot_matches(&job.project_snapshot) {
            self.finalize_export_job(
                ctx,
                job,
                Err("Project changed during export; the partial render was discarded".into()),
            );
            return;
        }
        if self.playhead_ms != job.next_frame_ms.min(self.timeline_end_ms()) || self.is_playing {
            self.finalize_export_job(
                ctx,
                job,
                Err(
                    "Playback position changed during export; the partial render was discarded"
                        .into(),
                ),
            );
            return;
        }

        if job.progress.cancel_requested() {
            self.finalize_export_job(ctx, job, Err("Export canceled".to_string()));
            return;
        }

        if !job.pending_capture {
            self.active_export = Some(job);
            return;
        }

        if !job.capture_ready {
            job.capture_ready = true;
            self.active_export = Some(job);
            ctx.request_repaint();
            return;
        }

        let missing_clips: Vec<_> = self
            .compositing_clips_in_render_order()
            .filter(|clip| clip.video_enabled && self.fresh_export_frame_for_clip(clip).is_none())
            .map(|clip| clip.id)
            .collect();
        if !missing_clips.is_empty() {
            if job.frame_wait_started_at.elapsed() >= std::time::Duration::from_secs(5) {
                self.finalize_export_job(ctx, job, Err(format!(
                    "Timed out waiting for fresh decoded frame(s) for clip(s) {missing_clips:?} at {} ms",
                    self.playhead_ms,
                )));
            } else {
                self.active_export = Some(job);
                ctx.request_repaint_after(std::time::Duration::from_millis(25));
            }
            return;
        }

        let capture_result = (|| {
            let (source_width, source_height) = self.capture_composited_preview_rgba_buffer()?;
            let export_pixels = Self::resize_rgba_buffer(
                &self.save_frame_buffer,
                source_width,
                source_height,
                job.settings.width,
                job.settings.height,
            )?;
            job.exporter.push_frame(&export_pixels)?;
            job.progress.mark_frame_complete();
            job.pending_capture = false;
            job.capture_ready = false;
            job.next_frame_index = job.next_frame_index.saturating_add(1);
            job.next_frame_ms = export_frame_time_ms(
                job.next_frame_index,
                job.settings.fps_num,
                job.settings.fps_den,
            );
            Ok(())
        })();

        if let Err(err) = capture_result {
            self.finalize_export_job(ctx, job, Err(err));
            return;
        }

        if job.progress.is_finished() {
            let finalize_result = (|| {
                job.exporter.end_of_stream()?;
                Ok(())
            })();
            self.finalize_export_job(ctx, job, finalize_result);
        } else {
            self.active_export = Some(job);
            ctx.request_repaint();
        }
    }
}
