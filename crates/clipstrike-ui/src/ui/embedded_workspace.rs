use super::{AppConfig, ClipStrikeApp};
use clipstrike_core::project::Project;
use std::path::Path;
use std::sync::Arc;

/// Owned host resources captured once during eframe creation. Clone and keep
/// this handle to open projects after the creation callback has returned.
#[derive(Clone)]
pub struct EditorHostResources {
    egui_ctx: egui::Context,
    wgpu_render_state: eframe::egui_wgpu::RenderState,
}

impl EditorHostResources {
    pub fn capture(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let wgpu_render_state = cc.wgpu_render_state.clone().ok_or_else(|| {
            "Embedded ClipStrike editor requires eframe's WGPU renderer".to_string()
        })?;
        Ok(Self {
            egui_ctx: cc.egui_ctx.clone(),
            wgpu_render_state,
        })
    }

    /// Open a project at any later point in the host's lifetime.
    pub fn open_project(&self, path: &Path) -> Result<EmbeddedEditorWorkspace, String> {
        EmbeddedEditorWorkspace::from_app(
            ClipStrikeApp::new_with_owned_render_state(
                &self.egui_ctx,
                Some(self.wgpu_render_state.clone()),
                AppConfig::default(),
                true,
            ),
            path,
        )
    }
}

struct RestoreHostStyle<'a> {
    ctx: &'a egui::Context,
    style: Arc<egui::Style>,
}

impl Drop for RestoreHostStyle<'_> {
    fn drop(&mut self) {
        self.ctx.set_style(self.style.clone());
    }
}

/// Full-frame editor workspace for another eframe host, such as DemoStrike.
///
/// The host must create its window with the WGPU renderer, keep this value alive
/// while the project is open, and delegate the editing frame to `show` instead
/// of drawing a second set of root egui panels. ClipStrike's standalone app
/// calls the same workspace method.
pub struct EmbeddedEditorWorkspace {
    app: ClipStrikeApp,
}

impl EmbeddedEditorWorkspace {
    /// Open a saved ClipStrike project without using the standalone app's
    /// recent-project list, theme, fullscreen, or configuration persistence.
    pub fn open(cc: &eframe::CreationContext<'_>, path: &Path) -> Result<Self, String> {
        EditorHostResources::capture(cc)?.open_project(path)
    }

    pub(crate) fn from_app(mut app: ClipStrikeApp, path: &Path) -> Result<Self, String> {
        if app.has_unsaved_changes() {
            return Err("Cannot replace an editor with unsaved changes".into());
        }
        app.embedded_workspace = true;
        app.app_config = AppConfig::default();
        app.open_project_unchecked(path.to_path_buf())?;
        Ok(Self { app })
    }

    /// Draw the same editor workspace that the standalone ClipStrike app uses.
    pub fn show(&mut self, ctx: &egui::Context) {
        let original_style = ctx.style();
        let mut editor_style = (*original_style).clone();
        editor_style.visuals = match self.app.app_config.theme {
            super::ThemePreference::Dark => egui::Visuals::dark(),
            super::ThemePreference::Light => egui::Visuals::light(),
        };
        for font_id in editor_style.text_styles.values_mut() {
            font_id.size *= 1.15;
        }
        ctx.set_style(editor_style);
        let _restore = RestoreHostStyle {
            ctx,
            style: original_style,
        };
        self.app.show_workspace(ctx);
    }

    pub fn project_path(&self) -> Option<&Path> {
        self.app.current_project_path.as_deref()
    }

    pub fn project_snapshot(&self) -> Project {
        self.app.snapshot_project()
    }

    /// A host should retain this workspace or confirm a discard before closing
    /// its editor view when this returns true.
    pub fn has_unsaved_changes(&self) -> bool {
        self.app.has_unsaved_changes()
    }

    /// Keep showing this workspace while true so its seek/capture/finalization
    /// steps and Cancel Export control continue to run.
    pub fn is_exporting(&self) -> bool {
        let active = self.app.active_export.is_some();
        #[cfg(not(target_arch = "wasm32"))]
        {
            active
                || self.app.pending_export_preparation.is_some()
                || self.app.pending_export_finalization.is_some()
        }
        #[cfg(target_arch = "wasm32")]
        {
            active
        }
    }

    /// Consume a File > Return to Capture request. The host should consult
    /// `has_unsaved_changes` and `is_exporting` before discarding this workspace.
    pub fn take_return_request(&mut self) -> bool {
        std::mem::take(&mut self.app.return_to_host_requested)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::ui::{PendingExportFinalization, PendingExportPreparation};
    use clipstrike_media::export::ExportSettings;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn repeated_editor_frames_preserve_host_style() {
        let ctx = egui::Context::default();
        let mut host_style = (*ctx.style()).clone();
        host_style.visuals = egui::Visuals::light();
        host_style
            .text_styles
            .get_mut(&egui::TextStyle::Body)
            .unwrap()
            .size = 17.0;
        ctx.set_style(host_style);
        let mut workspace = EmbeddedEditorWorkspace {
            app: ClipStrikeApp::test_harness(Project::new("Working", 320, 180, 10.0)),
        };
        workspace.app.embedded_workspace = true;
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| workspace.show(ctx));
            assert!(!ctx.style().visuals.dark_mode);
            assert_eq!(ctx.style().text_styles[&egui::TextStyle::Body].size, 17.0);
        }
    }

    #[test]
    fn host_can_see_when_editor_export_preparation_is_active() {
        let mut workspace = EmbeddedEditorWorkspace {
            app: ClipStrikeApp::test_harness(Project::new("Working", 320, 180, 10.0)),
        };
        assert!(!workspace.is_exporting());

        let (_, receiver) = std::sync::mpsc::channel();
        workspace.app.pending_export_preparation = Some(PendingExportPreparation {
            output_path: PathBuf::from("pending.mp4"),
            settings: ExportSettings::default(),
            total_frames: 1,
            project_snapshot: workspace.app.snapshot_project(),
            clips_snapshot: Vec::new(),
            receiver,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        assert!(workspace.is_exporting());
    }

    #[test]
    fn dropping_editor_cancels_pending_export_and_discards_queued_staging() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("unpublished.partial");
        let destination = dir.path().join("movie.mp4");
        std::fs::write(&staging, b"unpublished output").unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut workspace = EmbeddedEditorWorkspace {
            app: ClipStrikeApp::test_harness(Project::new("Working", 320, 180, 10.0)),
        };
        workspace.app.pending_export_finalization = Some(PendingExportFinalization {
            output_path: destination.clone(),
            project_snapshot: workspace.app.snapshot_project(),
            receiver,
            cancel: Arc::clone(&cancel),
        });
        sender
            .send(Ok(
                crate::audio_export::PreparedExport::fixture_from_staging(staging.clone()),
            ))
            .unwrap();

        drop(workspace);
        assert!(cancel.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!staging.exists());
        assert!(!destination.exists());
    }

    #[cfg(feature = "gstreamer-backend")]
    #[test]
    fn dropping_editor_stops_active_encoder_and_removes_its_stage() {
        use clipstrike_core::timeline::Clip;
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("unfinished.mp4");
        let mut app = ClipStrikeApp::test_harness(Project::new("Working", 320, 180, 10.0));
        app.timeline.clips.push(Clip::new(1, 1, 1, 0, 100, 0, 100));
        let settings = ExportSettings {
            width: 320,
            height: 180,
            fps_num: 10,
            fps_den: 1,
            use_hardware_acceleration: false,
            ..Default::default()
        };
        app.start_export_with_mix(
            &egui::Context::default(),
            destination.clone(),
            settings,
            1,
            None,
        )
        .unwrap();
        let stage = app.active_export.as_ref().unwrap().staging_path.clone();
        assert!(stage.exists());

        drop(EmbeddedEditorWorkspace { app });
        assert!(!stage.exists());
        assert!(!destination.exists());
    }

    #[cfg(all(windows, feature = "gstreamer-backend"))]
    #[test]
    fn owned_host_resources_open_after_startup_and_release_textures_on_repeated_close() {
        let instance = wgpu::Instance::default();
        let adapter =
            futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
            .expect("native WGPU adapter");
        let (device, queue) = futures::executor::block_on(
            adapter.request_device(&wgpu::DeviceDescriptor::default(), None),
        )
        .expect("native WGPU device");
        let target_format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let renderer = eframe::egui_wgpu::Renderer::new(&device, target_format, None, 1, false);
        let render_state = eframe::egui_wgpu::RenderState {
            adapter: Arc::new(adapter),
            available_adapters: Arc::from(Vec::<wgpu::Adapter>::new()),
            device: Arc::new(device),
            queue: Arc::new(queue),
            target_format,
            renderer: Arc::new(egui::epaint::mutex::RwLock::new(renderer)),
        };
        let ctx = egui::Context::default();
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::light();
        style
            .text_styles
            .get_mut(&egui::TextStyle::Body)
            .unwrap()
            .size = 19.0;
        ctx.set_style(style);
        let resources = EditorHostResources {
            egui_ctx: ctx.clone(),
            wgpu_render_state: render_state,
        };
        let dir = tempfile::tempdir().unwrap();
        let project_path = dir.path().join("host-open.cstrike");
        Project::new("Opened later", 320, 180, 10.0)
            .save(&project_path)
            .unwrap();

        for _ in 0..3 {
            let mut workspace = resources.open_project(&project_path).unwrap();
            assert_eq!(workspace.project_snapshot().name, "Opened later");
            assert!(!ctx.style().visuals.dark_mode);
            assert_eq!(ctx.style().text_styles[&egui::TextStyle::Body].size, 19.0);
            let mut ids = workspace
                .app
                .frame_renderer
                .as_ref()
                .unwrap()
                .texture_pool
                .get_frames()
                .iter()
                .filter_map(|frame| frame.egui_id)
                .collect::<Vec<_>>();
            ids.push(workspace.app.fx_output_id.unwrap());
            ids.push(workspace.app.accumulator_id.unwrap());
            assert_eq!(ids.len(), 12);
            {
                let renderer = resources.wgpu_render_state.renderer.read();
                assert!(ids.iter().all(|id| renderer.texture(id).is_some()));
            }
            let _ = ctx.run(egui::RawInput::default(), |ctx| workspace.show(ctx));
            assert!(!ctx.style().visuals.dark_mode);
            assert_eq!(ctx.style().text_styles[&egui::TextStyle::Body].size, 19.0);
            drop(workspace);
            let renderer = resources.wgpu_render_state.renderer.read();
            assert!(ids.iter().all(|id| renderer.texture(id).is_none()));
        }
    }
}
