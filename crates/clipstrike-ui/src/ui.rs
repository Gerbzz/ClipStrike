use crate::app_config::{AppConfig, ProjectProfile, ThemePreference};
use clipstrike_audio::waveform::extract_waveform_sync;
use clipstrike_media::engine_manager::EngineManager;
use clipstrike_media::probe::probe_metadata_sync;
use clipstrike_render::fx::{FxPipeline, FxUniforms, CompositorPipeline, ComposeUniforms};
use clipstrike_render::gpu::{FrameTexture, TexturePool, read_texture_rgba};
use clipstrike_core::clip_defaults::timeline_clip_end_ms;
use clipstrike_core::commands::{
    AddClipCommand, MoveClipCommand, RemoveClipCommand, SeparateAudioCommand, TrimClipCommand,
    UndoStack,
};

use clipstrike_core::keyframes::{DEFAULT_KEYFRAME_SNAP_MS, InterpolationMode, KeyframedProperty};
use clipstrike_core::labels::{timeline_clip_label, timeline_clip_tooltip};
use clipstrike_core::project::{
    Marker, MarkerToggleResult, MediaAsset, Project, TransitionMaskAsset,
};
use clipstrike_core::timeline::{Clip, Timeline};
#[cfg(feature = "gstreamer-backend")]
const PREVIEW_IDLE_MESSAGE: &str = "No decoded frame available yet.";
#[cfg(not(feature = "gstreamer-backend"))]
const PREVIEW_IDLE_MESSAGE: &str =
    "Media backend disabled. Use run_clipstrike_gstreamer.cmd for real preview.";

use clipstrike_core::transitions::{
    MaskImageRef, NormalizedRect, Transition, TransitionType, default_fade_duration_ms,
    fade_in_points, fade_out_points, incoming_mask_alpha, sample_mask_value,
};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const TRACK_HEADER_WIDTH: f32 = 132.0;
const TIMELINE_CONTENT_OFFSET_X: f32 = TRACK_HEADER_WIDTH + 16.0;

#[derive(PartialEq, Debug)]
pub enum LeftPanelTab {
    ProjectFiles,
    Transitions,
    Effects,
    Titles,
    Emojis,
}

pub struct ClipStrikeApp {
    left_panel_tab: LeftPanelTab,
    zoom_level: f32,
    timeline: Timeline,
    app_config: AppConfig,
    project: Project,
    current_project_path: Option<PathBuf>,
    undo_stack: UndoStack,

    // Engine & GPU Integration
    device: Option<Arc<wgpu::Device>>,
    queue: Option<Arc<wgpu::Queue>>,
    engine_manager: Option<EngineManager>,
    audio_engine: Option<clipstrike_audio::PlaybackEngine>,
    fx_pipeline: Option<FxPipeline>,
    compositor_pipeline: Option<CompositorPipeline>,
    texture_pool: Option<Arc<Mutex<TexturePool>>>,
    _fx_output_texture: Option<wgpu::Texture>,
    fx_output_view: Option<wgpu::TextureView>,
    fx_output_id: Option<egui::TextureId>,
    
    // Compositor accumulators
    _accumulator_texture: Option<wgpu::Texture>,
    accumulator_view: Option<wgpu::TextureView>,
    accumulator_id: Option<egui::TextureId>,
    _scratch_texture: Option<wgpu::Texture>,
    scratch_view: Option<wgpu::TextureView>,

    // Playback & Selection state
    is_playing: bool,
    current_time_ms: i64,
    playhead_ms: i64,
    selected_clip_id: Option<usize>,
    selected_marker_id: Option<usize>,
    selected_keyframe: Option<SelectedKeyframe>,
    razor_active: bool,
    drag_start_times: Option<(i64, i64)>,
    drag_pointer_start_x: Option<f32>,
    drag_clip_id: Option<usize>,
    drag_preview_times: Option<(i64, i64)>,
    master_volume: f32, // Task 12.5

    // Title Editor state (Phase 13)
    show_title_editor: bool,
    editing_title: Option<clipstrike_core::project::TitleAsset>,

    // Trimming state
    trim_side: Option<TrimSide>,

    // Looping state (Task 6.7)
    loop_enabled: bool,
    loop_start_ms: i64,
    loop_end_ms: i64,

    // Media Browser state (Phase 7)
    asset_filter_text: String,
    asset_filter_index: usize,
    show_preferences_dialog: bool,
    show_profile_dialog: bool,
    track_name_edit_id: Option<usize>,
    track_name_buffer: String,

    // Test Hooks Bounding Boxes
    test_first_track_rect: Option<egui::Rect>,
    test_first_asset_rect: Option<egui::Rect>,
    test_first_clip_rect: Option<egui::Rect>,
    test_first_title_rect: Option<egui::Rect>,
    test_project_json_dump: Option<String>,
}

#[derive(Clone, Copy)]
struct DraggedTransition {
    transition_type: TransitionType,
    custom_mask_id: Option<usize>,
}

impl Default for DraggedTransition {
    fn default() -> Self {
        Self {
            transition_type: TransitionType::Crossfade,
            custom_mask_id: None,
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum TrimSide {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SelectedKeyframe {
    clip_id: usize,
    property: AnimatedProperty,
    time_ms: i64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnimatedProperty {
    Brightness,
    Contrast,
    Saturation,
    Grayscale,
    BlurRadius,
    Volume,
    PosX,
    PosY,
    ScaleX,
    ScaleY,
    Rotation,
    Opacity,
    Speed,
}

impl AnimatedProperty {
    const VISUALS: [Self; 5] = [
        Self::Brightness,
        Self::Contrast,
        Self::Saturation,
        Self::Grayscale,
        Self::BlurRadius,
    ];
    const AUDIO: [Self; 1] = [Self::Volume];
    const TRANSFORMS: [Self; 6] = [
        Self::PosX,
        Self::PosY,
        Self::ScaleX,
        Self::ScaleY,
        Self::Rotation,
        Self::Opacity,
    ];
    const TIME_REMAP: [Self; 1] = [Self::Speed];

    fn label(self) -> &'static str {
        match self {
            Self::Brightness => "Brightness",
            Self::Contrast => "Contrast",
            Self::Saturation => "Saturation",
            Self::Grayscale => "Grayscale",
            Self::BlurRadius => "Blur Radius",
            Self::Volume => "Volume",
            Self::PosX => "Position X",
            Self::PosY => "Position Y",
            Self::ScaleX => "Scale X",
            Self::ScaleY => "Scale Y",
            Self::Rotation => "Rotation",
            Self::Opacity => "Opacity",
            Self::Speed => "Speed",
        }
    }

    fn range(self) -> (f32, f32) {
        match self {
            Self::Brightness | Self::Contrast | Self::Saturation => (0.0, 5.0),
            Self::Grayscale | Self::Opacity => (0.0, 1.0),
            Self::BlurRadius => (0.0, 20.0),
            Self::Volume => (0.0, 2.0),
            Self::PosX | Self::PosY => (-1000.0, 1000.0),
            Self::ScaleX | Self::ScaleY => (0.0, 5.0),
            Self::Rotation => (-180.0, 180.0),
            Self::Speed => (0.0, 4.0),
        }
    }
}

fn property_ref(clip: &Clip, property: AnimatedProperty) -> &KeyframedProperty {
    match property {
        AnimatedProperty::Brightness => &clip.brightness,
        AnimatedProperty::Contrast => &clip.contrast,
        AnimatedProperty::Saturation => &clip.saturation,
        AnimatedProperty::Grayscale => &clip.grayscale,
        AnimatedProperty::BlurRadius => &clip.blur_radius,
        AnimatedProperty::Volume => &clip.volume,
        AnimatedProperty::PosX => &clip.pos_x,
        AnimatedProperty::PosY => &clip.pos_y,
        AnimatedProperty::ScaleX => &clip.scale_x,
        AnimatedProperty::ScaleY => &clip.scale_y,
        AnimatedProperty::Rotation => &clip.rotation,
        AnimatedProperty::Opacity => &clip.opacity,
        AnimatedProperty::Speed => &clip.speed,
    }
}

fn property_mut(clip: &mut Clip, property: AnimatedProperty) -> &mut KeyframedProperty {
    match property {
        AnimatedProperty::Brightness => &mut clip.brightness,
        AnimatedProperty::Contrast => &mut clip.contrast,
        AnimatedProperty::Saturation => &mut clip.saturation,
        AnimatedProperty::Grayscale => &mut clip.grayscale,
        AnimatedProperty::BlurRadius => &mut clip.blur_radius,
        AnimatedProperty::Volume => &mut clip.volume,
        AnimatedProperty::PosX => &mut clip.pos_x,
        AnimatedProperty::PosY => &mut clip.pos_y,
        AnimatedProperty::ScaleX => &mut clip.scale_x,
        AnimatedProperty::ScaleY => &mut clip.scale_y,
        AnimatedProperty::Rotation => &mut clip.rotation,
        AnimatedProperty::Opacity => &mut clip.opacity,
        AnimatedProperty::Speed => &mut clip.speed,
    }
}

impl ClipStrikeApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let app_config = AppConfig::load_or_default();
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = match app_config.theme {
            ThemePreference::Dark => egui::Visuals::dark(),
            ThemePreference::Light => egui::Visuals::light(),
        };
        cc.egui_ctx.set_style(style);
        if app_config.fullscreen {
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }

        let (
            device,
            queue,
            texture_pool,
            _fx_output_texture,
            fx_output_view,
            fx_output_id,
            fx_pipeline,
            compositor_pipeline,
            _accumulator_texture,
            accumulator_view,
            accumulator_id,
            _scratch_texture,
            scratch_view,
            engine_manager,
            audio_engine,
        ) = if let Some(wgpu_render_state) = cc.wgpu_render_state.as_ref() {
            let device = wgpu_render_state.device.clone();
            let queue = wgpu_render_state.queue.clone();

            let mut pool = TexturePool::new(
                device.clone(),
                wgpu::TextureFormat::Rgba8Unorm,
                1920,
                1080,
                10,
            );

            {
                let mut renderer = wgpu_render_state.renderer.write();
                for frame in pool.get_frames_mut() {
                    frame.egui_id = Some(renderer.register_native_texture(
                        &device,
                        &frame.view,
                        wgpu::FilterMode::Linear,
                    ));
                }
            }

            let texture_pool = Arc::new(Mutex::new(pool));

            let fx_output_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("FxPreviewTexture"),
                size: wgpu::Extent3d {
                    width: 1920,
                    height: 1080,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let fx_output_view = fx_output_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let fx_output_id = {
                let mut renderer = wgpu_render_state.renderer.write();
                renderer.register_native_texture(&device, &fx_output_view, wgpu::FilterMode::Linear)
            };

            let accum_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Compositor Accumulator"),
                size: wgpu::Extent3d {
                    width: 1920,
                    height: 1080,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let accum_view = accum_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let accum_id = {
                let mut renderer = wgpu_render_state.renderer.write();
                renderer.register_native_texture(&device, &accum_view, wgpu::FilterMode::Linear)
            };

            let scratch_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Compositor Scratch"),
                size: wgpu::Extent3d {
                    width: 1920,
                    height: 1080,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let scratch_view = scratch_texture.create_view(&wgpu::TextureViewDescriptor::default());
            
            let fx_pipeline = FxPipeline::new(&device);
            let compositor_pipeline = CompositorPipeline::new(&device);
            let engine_manager = EngineManager::new(Arc::clone(&texture_pool), queue.clone());
            let audio_engine = clipstrike_audio::PlaybackEngine::new().ok();

            (
                Some(device),
                Some(queue),
                Some(texture_pool),
                Some(fx_output_texture),
                Some(fx_output_view),
                Some(fx_output_id),
                Some(fx_pipeline),
                Some(compositor_pipeline),
                Some(accum_texture),
                Some(accum_view),
                Some(accum_id),
                Some(scratch_texture),
                Some(scratch_view),
                Some(engine_manager),
                audio_engine,
            )
        } else {
            (None, None, None, None, None, None, None, None, None, None, None, None, None, None, None)
        };

        let profile = app_config.default_project_profile.clone();
        let mut project = Project::new(
            "New Project",
            profile.width,
            profile.height,
            profile.framerate,
        );
        project.tracks = Project::default_tracks(5);
        let master_volume = project.master_volume;

        // Improve readability by increasing default font size
        let mut style = (*cc.egui_ctx.style()).clone();
        for font_id in style.text_styles.values_mut() {
            font_id.size *= 1.15; // 15% larger text everywhere
        }
        cc.egui_ctx.set_style(style);

        Self {
            left_panel_tab: LeftPanelTab::ProjectFiles,
            zoom_level: 0.5,
            timeline: Timeline::new(),
            app_config,
            project,
            current_project_path: None,
            undo_stack: UndoStack::new(),
            device,
            queue,
            engine_manager,
            audio_engine,
            fx_pipeline,
            compositor_pipeline,
            texture_pool,
            _fx_output_texture,
            fx_output_view,
            fx_output_id,
            _accumulator_texture,
            accumulator_view,
            accumulator_id,
            _scratch_texture,
            scratch_view,
            is_playing: false,
            current_time_ms: 0,
            playhead_ms: 0,
            selected_clip_id: None,
            selected_marker_id: None,
            selected_keyframe: None,
            razor_active: false,
            drag_start_times: None,
            drag_pointer_start_x: None,
            drag_clip_id: None,
            drag_preview_times: None,
            trim_side: None,
            test_first_track_rect: None,
            test_first_asset_rect: None,
            test_first_clip_rect: None,
            test_first_title_rect: None,
            test_project_json_dump: None,
            loop_enabled: false,
            loop_start_ms: 0,
            loop_end_ms: 10000,
            asset_filter_text: String::new(),
            asset_filter_index: 0,
            master_volume,
            show_preferences_dialog: false,
            show_profile_dialog: false,
            track_name_edit_id: None,
            track_name_buffer: String::new(),
            show_title_editor: false,
            editing_title: None,
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        style.visuals = match self.app_config.theme {
            ThemePreference::Dark => egui::Visuals::dark(),
            ThemePreference::Light => egui::Visuals::light(),
        };
        ctx.set_style(style);
    }

    fn default_project(&self) -> Project {
        let profile = &self.app_config.default_project_profile;
        let mut project = Project::new(
            "New Project",
            profile.width,
            profile.height,
            profile.framerate,
        );
        project.tracks = Project::default_tracks(5);
        project
    }

    fn apply_profile_to_project(&mut self, profile: &ProjectProfile) {
        self.project.width = profile.width;
        self.project.height = profile.height;
        self.project.framerate = profile.framerate;
    }

    fn persist_app_config(&self) {
        if let Err(err) = self.app_config.save() {
            log::error!("Failed to save app config: {}", err);
        }
    }

    fn remember_recent_project(&mut self, path: PathBuf) {
        self.app_config.remember_recent_project(path);
        self.persist_app_config();
    }

    fn snapshot_project(&self) -> Project {
        let mut project = self.project.clone();
        project.capture_timeline(&self.timeline);
        project
    }

    fn reset_runtime_state(&mut self, project: Project, path: Option<PathBuf>) {
        self.timeline = project.to_timeline();
        self.project = project;
        self.current_project_path = path;
        self.undo_stack = UndoStack::new();
        self.is_playing = false;
        self.current_time_ms = 0;
        self.playhead_ms = 0;
        self.selected_clip_id = None;
        self.selected_marker_id = None;
        self.selected_keyframe = None;
        self.track_name_edit_id = None;
        self.track_name_buffer.clear();
        self.razor_active = false;
        self.drag_start_times = None;
        self.drag_pointer_start_x = None;
        self.drag_clip_id = None;
        self.drag_preview_times = None;
        self.trim_side = None;
        if let Some(engine_manager) = &mut self.engine_manager {
            engine_manager.remove_unused(&[]);
        }
    }

    fn new_project(&mut self) {
        self.reset_runtime_state(self.default_project(), None);
    }

    fn open_project_at_path(&mut self, path: PathBuf) -> Result<(), String> {
        let project = Project::load(&path).map_err(|err| err.to_string())?;
        self.reset_runtime_state(project, Some(path.clone()));
        self.remember_recent_project(path);
        Ok(())
    }

    fn open_project_dialog(&mut self) -> Result<(), String> {
        if let Some(path) = self.pick_project_file() {
            self.open_project_at_path(path)?;
        }
        Ok(())
    }

    fn save_project_to_path(&mut self, path: PathBuf) -> Result<(), String> {
        let project = self.snapshot_project();
        project.save(&path).map_err(|err| err.to_string())?;
        self.project = project;
        self.current_project_path = Some(path.clone());
        self.remember_recent_project(path);
        Ok(())
    }

    fn save_project(&mut self) -> Result<(), String> {
        if let Some(path) = self.current_project_path.clone() {
            self.save_project_to_path(path)
        } else {
            self.save_project_as()
        }
    }

    fn save_project_as(&mut self) -> Result<(), String> {
        if let Some(path) = self.pick_project_save_path() {
            self.save_project_to_path(path)?;
        }
        Ok(())
    }


    #[cfg(not(target_arch = "wasm32"))]
    fn pick_project_file(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("ClipStrike Project", &["cstrike"])
            .pick_file()
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_project_file(&self) -> Option<PathBuf> {
        log::warn!("Open project dialog is not available on web builds.");
        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_project_save_path(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("ClipStrike Project", &["cstrike"])
            .set_file_name("project.cstrike")
            .save_file()
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_project_save_path(&self) -> Option<PathBuf> {
        log::warn!("Save-as dialog is not available on web builds.");
        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_media_files(&self) -> Option<Vec<PathBuf>> {
        rfd::FileDialog::new()
            .add_filter(
                "Video/Audio",
                &["mp4", "mkv", "mov", "wav", "mp3", "png", "jpg"],
            )
            .pick_files()
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_media_files(&self) -> Option<Vec<PathBuf>> {
        log::warn!("Import dialog is not available on web builds.");
        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_luma_mask_file(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
            .pick_file()
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_luma_mask_file(&self) -> Option<PathBuf> {
        log::warn!("Luma mask import dialog is not available on web builds.");
        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_frame_save_path(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg"])
            .save_file()
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_frame_save_path(&self) -> Option<PathBuf> {
        log::warn!("Save-frame dialog is not available on web builds.");
        None
    }

    fn import_media_path(&mut self, path: PathBuf) {
        let uri = format!("file:///{}", path.to_string_lossy().replace("\\", "/"));
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Imported Media")
            .to_string();
        let duration_ms = match probe_metadata_sync(&uri) {
            Ok(metadata) => metadata
                .duration
                .map(|duration: std::time::Duration| {
                    duration.as_millis().min(u64::MAX as u128) as u64
                })
                .unwrap_or(0),
            Err(err) => {
                log::warn!("Failed to probe media metadata for {}: {}", uri, err);
                0
            }
        };
        let audio_levels = match extract_waveform_sync(&uri, 96) {
            Ok(levels) => levels,
            Err(err) => {
                log::warn!("Failed to extract waveform for {}: {}", uri, err);
                Vec::new()
            }
        };

        self.project.assets.push(MediaAsset {
            id: rand::random::<usize>(),
            name,
            uri,
            duration_ms,
            audio_levels,
        });
    }

    fn import_media_paths<I>(&mut self, paths: I)
    where
        I: IntoIterator<Item = PathBuf>,
    {
        for path in paths {
            self.import_media_path(path);
        }
    }

    fn apply_track_mutation<F>(&mut self, mutate: F) -> Result<(), String>
    where
        F: FnOnce(&mut Project) -> Result<(), String>,
    {
        self.project.capture_timeline(&self.timeline);
        mutate(&mut self.project)?;
        self.timeline = self.project.to_timeline();
        if let Some(selected_id) = self.selected_clip_id {
            if !self
                .timeline
                .clips
                .iter()
                .any(|clip| clip.id == selected_id)
            {
                self.selected_clip_id = None;
                self.selected_keyframe = None;
            }
        }
        Ok(())
    }

    fn frame_duration_ms(&self) -> i64 {
        (1000.0 / self.project.framerate.max(1.0)).round() as i64
    }

    fn format_timecode(&self, time_ms: i64) -> String {
        let fps = self.project.framerate.max(1.0);
        let total_ms = time_ms.max(0);
        let hours = total_ms / 3_600_000;
        let minutes = (total_ms / 60_000) % 60;
        let seconds = (total_ms / 1_000) % 60;
        let frames = (((total_ms % 1_000) as f64 / 1_000.0) * fps).floor() as i64;
        format!("{:02}:{:02}:{:02}:{:02}", hours, minutes, seconds, frames)
    }

    fn timeline_end_ms(&self) -> i64 {
        self.timeline
            .clips
            .iter()
            .map(|clip| clip.end)
            .max()
            .unwrap_or(0)
    }

    fn apply_fade_in(clip: &mut Clip) {
        for (time_ms, value) in fade_in_points(clip.start, clip.end) {
            clip.opacity
                .insert_keyframe(time_ms, value, InterpolationMode::Linear);
        }
    }

    fn apply_fade_out(clip: &mut Clip) {
        for (time_ms, value) in fade_out_points(clip.start, clip.end) {
            clip.opacity
                .insert_keyframe(time_ms, value, InterpolationMode::Linear);
        }
    }

    fn normalized_rect(rect: egui::Rect, normalized: NormalizedRect) -> egui::Rect {
        egui::Rect::from_min_max(
            egui::pos2(
                egui::lerp(rect.left()..=rect.right(), normalized.min_x),
                egui::lerp(rect.top()..=rect.bottom(), normalized.min_y),
            ),
            egui::pos2(
                egui::lerp(rect.left()..=rect.right(), normalized.max_x),
                egui::lerp(rect.top()..=rect.bottom(), normalized.max_y),
            ),
        )
    }

    fn preview_texture_id(
        &self,
        clip: &Clip,
        frame: &FrameTexture,
    ) -> Option<egui::TextureId> {
        let texture_id = frame.egui_id?;
        
        let (device, queue, fx_pipeline, fx_output_view, fx_output_id) = match (
            &self.device,
            &self.queue,
            &self.fx_pipeline,
            &self.fx_output_view,
            &self.fx_output_id,
        ) {
            (Some(d), Some(q), Some(fp), Some(fov), Some(fid)) => (d, q, fp, fov, fid),
            _ => return Some(texture_id), // Fallback to raw texture if no GPU fx
        };

        let params = FxUniforms {
            brightness: clip.brightness.get_value_at(self.playhead_ms),
            contrast: clip.contrast.get_value_at(self.playhead_ms),
            saturation: clip.saturation.get_value_at(self.playhead_ms),
            grayscale: clip.grayscale.get_value_at(self.playhead_ms),
            blur_radius: clip.blur_radius.get_value_at(self.playhead_ms),
            alpha: clip.opacity.get_value_at(self.playhead_ms),
            ..Default::default()
        };

        fx_pipeline.run(
            device,
            queue,
            &frame.view,
            fx_output_view,
            params,
        );
        Some(*fx_output_id)
    }

    fn transition_mask_image<'a>(&'a self, transition: &Transition) -> Option<MaskImageRef<'a>> {
        let mask_id = transition.custom_mask_id?;
        let mask = self
            .project
            .transition_masks
            .iter()
            .find(|candidate| candidate.id == mask_id)?;
        Some(MaskImageRef {
            width: mask.width,
            height: mask.height,
            pixels: &mask.pixels,
        })
    }

    fn draw_transition_property_control(
        ui: &mut egui::Ui,
        _transition_id: usize,
        label: &str,
        property: &mut KeyframedProperty,
        playhead_ms: i64,
        range: std::ops::RangeInclusive<f32>,
        snap_points: &[i64],
    ) {
        let (min, max) = (*range.start(), *range.end());
        ui.horizontal(|ui| {
            ui.label(label);
            let mut value = property.get_value_at(playhead_ms).clamp(min, max);
            if ui
                .add(egui::Slider::new(&mut value, range.clone()).show_value(true))
                .changed()
            {
                property.insert_keyframe_snapped(
                    playhead_ms,
                    value,
                    InterpolationMode::Linear,
                    DEFAULT_KEYFRAME_SNAP_MS,
                    snap_points,
                );
            }
            if ui.button("Key").clicked() {
                property.insert_keyframe_snapped(
                    playhead_ms,
                    value,
                    InterpolationMode::Linear,
                    DEFAULT_KEYFRAME_SNAP_MS,
                    snap_points,
                );
            }
            ui.label(format!("{} keys", property.keyframes.len()));
        });
    }

    fn draw_masked_transition_preview(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        outgoing_texture: egui::TextureId,
        incoming_texture: egui::TextureId,
        transition: &Transition,
        outgoing_alpha: f32,
        incoming_alpha: f32,
    ) {
        ui.painter().image(
            outgoing_texture,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::from_white_alpha((outgoing_alpha.clamp(0.0, 1.0) * 255.0) as u8),
        );

        let progress = transition.progress_at(self.playhead_ms);
        let softness = transition.softness_at(self.playhead_ms);
        let custom_mask = self.transition_mask_image(transition);
        let cols = 24;
        let rows = 14;
        let unit_rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

        for row in 0..rows {
            for col in 0..cols {
                let min_x = col as f32 / cols as f32;
                let max_x = (col + 1) as f32 / cols as f32;
                let min_y = row as f32 / rows as f32;
                let max_y = (row + 1) as f32 / rows as f32;
                let center_x = (min_x + max_x) * 0.5;
                let center_y = (min_y + max_y) * 0.5;
                let mask_value =
                    sample_mask_value(transition.transition_type, custom_mask, center_x, center_y);
                let alpha = incoming_mask_alpha(mask_value, progress, softness) * incoming_alpha;
                if alpha <= 0.01 {
                    continue;
                }

                let tile = NormalizedRect {
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                };
                ui.painter().image(
                    incoming_texture,
                    Self::normalized_rect(rect, tile),
                    Self::normalized_rect(unit_rect, tile),
                    egui::Color32::from_white_alpha((alpha.clamp(0.0, 1.0) * 255.0) as u8),
                );
            }
        }
    }
    fn save_current_frame_to_path(&self, path: &std::path::Path) -> Result<(), String> {
        let texture_pool = self.texture_pool.as_ref().ok_or_else(|| "WGPU state missing".to_string())?;
        let queue = self.queue.as_ref().ok_or_else(|| "WGPU state missing".to_string())?;
        let device = self.device.as_ref().ok_or_else(|| "WGPU state missing".to_string())?;
        let fx_output_texture = self._fx_output_texture.as_ref().ok_or_else(|| "WGPU state missing".to_string())?;

        let (frame_index, width, height) = {
            let pool = texture_pool
                .lock()
                .map_err(|_| "Failed to lock texture pool".to_string())?;
            let frame = pool
                .get_frames()
                .iter()
                .find(|texture| texture.ready)
                .ok_or_else(|| "No frame available to export".to_string())?;
            (frame.index, pool.width, pool.height)
        };

        let pixels = if self.selected_clip_id.is_some() {
            read_texture_rgba(
                device,
                queue,
                fx_output_texture,
                width,
                height,
            )?
        } else {
            let pool = texture_pool
                .lock()
                .map_err(|_| "Failed to lock texture pool".to_string())?;
            pool.read_frame(queue, frame_index)?
        };

        let image = image::RgbaImage::from_raw(width, height, pixels)
            .ok_or_else(|| "Failed to assemble image buffer".to_string())?;
        image
            .save(path)
            .map_err(|err| format!("Failed to save frame: {err}"))
    }

    fn marker_color(marker: &Marker) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(
            marker.color[0],
            marker.color[1],
            marker.color[2],
            marker.color[3],
        )
    }

    fn set_marker_color(marker: &mut Marker, color: egui::Color32) {
        marker.color = [color.r(), color.g(), color.b(), color.a()];
    }

    fn draw_property_section(
        ui: &mut egui::Ui,
        clip: &mut Clip,
        clip_id: usize,
        playhead_ms: i64,
        selected_keyframe: &mut Option<SelectedKeyframe>,
        title: &str,
        properties: &[AnimatedProperty],
    ) {
        ui.separator();
        ui.label(title);
        for property in properties {
            Self::draw_keyframed_property_editor(
                ui,
                clip,
                clip_id,
                *property,
                playhead_ms,
                selected_keyframe,
            );
        }
    }

    fn draw_keyframed_property_editor(
        ui: &mut egui::Ui,
        clip: &mut Clip,
        clip_id: usize,
        property: AnimatedProperty,
        playhead_ms: i64,
        selected_keyframe: &mut Option<SelectedKeyframe>,
    ) {
        let (min, max) = property.range();
        let label = property.label();
        let clip_start = clip.start;
        let clip_end = clip.end.max(clip.start + 1);
        let selected_for_property = selected_keyframe
            .as_ref()
            .filter(|selected| selected.clip_id == clip_id && selected.property == property)
            .map(|selected| selected.time_ms);

        {
            let prop = property_mut(clip, property);
            ui.horizontal(|ui| {
                ui.label(label);
                let mut value = prop.get_value_at(playhead_ms).clamp(min, max);
                if ui
                    .add(egui::Slider::new(&mut value, min..=max).show_value(true))
                    .changed()
                {
                    let snapped_time = prop.insert_keyframe_snapped(
                        playhead_ms,
                        value,
                        InterpolationMode::Linear,
                        DEFAULT_KEYFRAME_SNAP_MS,
                        &[playhead_ms],
                    );
                    *selected_keyframe = Some(SelectedKeyframe {
                        clip_id,
                        property,
                        time_ms: snapped_time,
                    });
                }
                if ui
                    .button("Key")
                    .on_hover_text("Insert keyframe at playhead")
                    .clicked()
                {
                    let snapped_time = prop.insert_keyframe_snapped(
                        playhead_ms,
                        value,
                        InterpolationMode::Linear,
                        DEFAULT_KEYFRAME_SNAP_MS,
                        &[playhead_ms],
                    );
                    *selected_keyframe = Some(SelectedKeyframe {
                        clip_id,
                        property,
                        time_ms: snapped_time,
                    });
                }
            });
        }

        let (curve_rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 96.0),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter();
        painter.rect_filled(curve_rect, 4.0, ui.visuals().faint_bg_color);
        painter.rect_stroke(
            curve_rect,
            4.0,
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );

        let duration_ms = (clip_end - clip_start).max(1);
        let to_x = |time_ms: i64| {
            curve_rect.left()
                + (((time_ms - clip_start) as f32 / duration_ms as f32).clamp(0.0, 1.0)
                    * curve_rect.width())
        };
        let to_y = |value: f32| {
            let normalized = ((value - min) / (max - min).max(f32::EPSILON)).clamp(0.0, 1.0);
            curve_rect.bottom() - normalized * curve_rect.height()
        };
        let from_x = |x: f32| {
            clip_start
                + (((x - curve_rect.left()) / curve_rect.width()).clamp(0.0, 1.0)
                    * duration_ms as f32)
                    .round() as i64
        };
        let from_y = |y: f32| {
            let normalized = ((curve_rect.bottom() - y) / curve_rect.height()).clamp(0.0, 1.0);
            min + normalized * (max - min)
        };

        for grid_step in 1..4 {
            let t = grid_step as f32 / 4.0;
            let y = egui::lerp(curve_rect.bottom()..=curve_rect.top(), t);
            painter.line_segment(
                [
                    egui::pos2(curve_rect.left(), y),
                    egui::pos2(curve_rect.right(), y),
                ],
                egui::Stroke::new(
                    1.0,
                    ui.visuals()
                        .widgets
                        .noninteractive
                        .bg_stroke
                        .color
                        .linear_multiply(0.35),
                ),
            );
        }

        let playhead_x = to_x(playhead_ms.clamp(clip_start, clip_end));
        painter.line_segment(
            [
                egui::pos2(playhead_x, curve_rect.top()),
                egui::pos2(playhead_x, curve_rect.bottom()),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 80, 80)),
        );

        let sample_count = 48;
        let prop_snapshot = property_ref(clip, property).clone();
        let mut points = Vec::with_capacity((sample_count + 1) as usize);
        for sample in 0..=sample_count {
            let time_ms = clip_start + (duration_ms * sample as i64) / sample_count as i64;
            points.push(egui::pos2(
                to_x(time_ms),
                to_y(prop_snapshot.get_value_at(time_ms)),
            ));
        }
        painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 200, 140)),
        ));

        let pointer_pos = response.interact_pointer_pos();
        let hit_time = pointer_pos.and_then(|pos| {
            prop_snapshot
                .keyframes
                .iter()
                .map(|keyframe| {
                    let point = egui::pos2(to_x(keyframe.time_ms), to_y(keyframe.value));
                    (keyframe.time_ms, point.distance(pos))
                })
                .filter(|(_, distance)| *distance <= 8.0)
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(time_ms, _)| time_ms)
        });

        for keyframe in &prop_snapshot.keyframes {
            let point = egui::pos2(to_x(keyframe.time_ms), to_y(keyframe.value));
            let is_selected = selected_for_property == Some(keyframe.time_ms);
            let fill = if is_selected {
                egui::Color32::from_rgb(255, 170, 0)
            } else {
                egui::Color32::from_rgb(220, 220, 220)
            };
            painter.circle_filled(point, if is_selected { 5.5 } else { 4.0 }, fill);
        }

        if response.secondary_clicked() {
            if let Some(time_ms) = hit_time {
                let prop = property_mut(clip, property);
                prop.remove_keyframe(time_ms);
                if selected_for_property == Some(time_ms) {
                    *selected_keyframe = None;
                }
            }
        } else if response.double_clicked() {
            if let Some(pos) = pointer_pos {
                let prop = property_mut(clip, property);
                let snapped_time = prop.insert_keyframe_snapped(
                    from_x(pos.x),
                    from_y(pos.y).clamp(min, max),
                    InterpolationMode::Linear,
                    DEFAULT_KEYFRAME_SNAP_MS,
                    &[playhead_ms],
                );
                *selected_keyframe = Some(SelectedKeyframe {
                    clip_id,
                    property,
                    time_ms: snapped_time,
                });
            }
        } else if response.clicked() {
            *selected_keyframe = hit_time.map(|time_ms| SelectedKeyframe {
                clip_id,
                property,
                time_ms,
            });
        }

        if response.dragged() {
            if let Some(pos) = pointer_pos {
                let active_time = hit_time.or(selected_for_property);
                if let Some(time_ms) = active_time {
                    let prop = property_mut(clip, property);
                    if let Ok(snapped_time) = prop.move_keyframe(
                        time_ms,
                        from_x(pos.x),
                        DEFAULT_KEYFRAME_SNAP_MS,
                        &[playhead_ms],
                    ) {
                        let mode = prop
                            .keyframe(snapped_time)
                            .map(|keyframe| keyframe.mode)
                            .unwrap_or(InterpolationMode::Linear);
                        let _ =
                            prop.update_keyframe(snapped_time, from_y(pos.y).clamp(min, max), mode);
                        *selected_keyframe = Some(SelectedKeyframe {
                            clip_id,
                            property,
                            time_ms: snapped_time,
                        });
                    }
                }
            }
        }

        if let Some(time_ms) = selected_for_property {
            if let Some(existing) = property_ref(clip, property).keyframe(time_ms).cloned() {
                let mut new_time = existing.time_ms;
                let mut new_value = existing.value;
                let mut new_mode = existing.mode;
                let mut delete_selected = false;
                ui.horizontal(|ui| {
                    ui.label("Selected Keyframe");
                    ui.add(egui::DragValue::new(&mut new_time).speed(10));
                    ui.add(egui::Slider::new(&mut new_value, min..=max).show_value(true));
                    egui::ComboBox::from_id_salt((
                        "keyframe_mode",
                        clip_id,
                        property as u8,
                        time_ms,
                    ))
                    .selected_text(match new_mode {
                        InterpolationMode::Linear => "Linear",
                        InterpolationMode::Bezier => "Bezier",
                        InterpolationMode::Constant => "Constant",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut new_mode, InterpolationMode::Linear, "Linear");
                        ui.selectable_value(&mut new_mode, InterpolationMode::Bezier, "Bezier");
                        ui.selectable_value(&mut new_mode, InterpolationMode::Constant, "Constant");
                    });
                    if ui.button("Delete").clicked() {
                        delete_selected = true;
                    }
                });

                let prop = property_mut(clip, property);
                if delete_selected {
                    prop.remove_keyframe(time_ms);
                    *selected_keyframe = None;
                } else if new_time != existing.time_ms
                    || (new_value - existing.value).abs() > f32::EPSILON
                    || new_mode != existing.mode
                {
                    let final_time = if new_time != existing.time_ms {
                        prop.move_keyframe(
                            existing.time_ms,
                            new_time,
                            DEFAULT_KEYFRAME_SNAP_MS,
                            &[playhead_ms],
                        )
                        .unwrap_or(existing.time_ms)
                    } else {
                        existing.time_ms
                    };
                    let _ = prop.update_keyframe(final_time, new_value, new_mode);
                    *selected_keyframe = Some(SelectedKeyframe {
                        clip_id,
                        property,
                        time_ms: final_time,
                    });
                }
            }
        }
    }

    fn toggle_marker_at_playhead(&mut self) {
        match self.project.toggle_marker(self.playhead_ms) {
            MarkerToggleResult::Added(marker) => {
                self.selected_marker_id = Some(marker.id);
            }
            MarkerToggleResult::Removed(marker) => {
                if self.selected_marker_id == Some(marker.id) {
                    self.selected_marker_id = None;
                }
            }
        }
    }

    fn active_clips_for_playhead(&self) -> (Vec<Clip>, Vec<Clip>) {
        let all_active_clips = self.timeline.get_clips_at_time(self.playhead_ms);
        let visible_active_clips = all_active_clips
            .iter()
            .filter(|clip| {
                self.project
                    .tracks
                    .iter()
                    .any(|track| track.id == clip.track_id && track.visible)
            })
            .cloned()
            .collect();
        (all_active_clips, visible_active_clips)
    }

    fn blend_mode_to_u32(mode: clipstrike_core::timeline::BlendMode) -> u32 {
        use clipstrike_core::timeline::BlendMode;
        match mode {
            BlendMode::Normal => 0,
            BlendMode::Multiply => 1,
            BlendMode::Screen => 2,
            BlendMode::Overlay => 3,
            BlendMode::Darken => 4,
            BlendMode::Lighten => 5,
            BlendMode::Add => 16,
            BlendMode::Subtract => 17,
            _ => 0, // Fallback to Normal
        }
    }

    fn sync_engines_for_playhead(&mut self, active_clips: &[Clip], seek_now: bool) {
        let engine_manager = match &mut self.engine_manager {
            Some(em) => em,
            None => return,
        };
        let active_ids: Vec<usize> = active_clips.iter().map(|clip| clip.id).collect();
        engine_manager.remove_unused(&active_ids);

        let mut audio_sources = Vec::new();

        let mut created_engine = false;
        for clip in active_clips {
            if let Some(asset) = self.project.assets.iter().find(|a| a.id == clip.asset_id) {
                match engine_manager.sync_clip(clip, &asset.uri) {
                    Ok(created) => created_engine |= created,
                    Err(err) => log::error!("Failed to sync clip {}: {}", clip.id, err),
                }
            }
            let volume = if self.project.is_track_audible(clip.track_id) {
                (clip.volume.get_value_at(self.playhead_ms) * self.master_volume).clamp(0.0, 2.0) as f64
            } else {
                0.0
            };

            engine_manager.set_clip_volume(clip.id, volume);

            // Synchronize Audio Engine (Task 12.5, 12.1)
            if self.project.is_track_audible(clip.track_id) && clip.audio_enabled {
                let samples = engine_manager.get_audio_samples(clip.id);

                audio_sources.push(clipstrike_audio::AudioSource {
                    clip_id: clip.id,
                    samples,
                    start_ms: clip.start,
                    end_ms: clip.end,
                    volume: clip.volume.clone(),
                });
            }
        }

        if let Some(audio) = &self.audio_engine {
            audio.set_sources(audio_sources);
        }

        if seek_now || created_engine {
            engine_manager.seek_clips(active_clips, self.playhead_ms);
        }

        if self.is_playing {
            engine_manager.play_clips(&active_ids);
        } else {
            engine_manager.pause_all();
        }
    }

    fn seek_playhead(&mut self) {
        let (all_active_clips, _) = self.active_clips_for_playhead();
        self.sync_engines_for_playhead(&all_active_clips, true);
    }

    fn jump_to_previous_marker(&mut self) {
        if let Some(marker) = self.project.previous_marker(self.playhead_ms).cloned() {
            self.playhead_ms = marker.time_ms;
            self.selected_marker_id = Some(marker.id);
            self.seek_playhead();
        }
    }

    fn jump_to_next_marker(&mut self) {
        if let Some(marker) = self.project.next_marker(self.playhead_ms).cloned() {
            self.playhead_ms = marker.time_ms;
            self.selected_marker_id = Some(marker.id);
            self.seek_playhead();
        }
    }

    fn draw_title_editor(&mut self, ctx: &egui::Context) {
        if !self.show_title_editor {
            return;
        }

        if self.editing_title.is_none() {
            self.editing_title = Some(clipstrike_core::project::TitleAsset {
                id: rand::random::<usize>(),
                name: "New Title".to_string(),
                template: clipstrike_core::project::TitleTemplate::default(),
                text: "ClipStrike".to_string(),
                font_family: "Arial".to_string(),
                font_size: 48.0,
                color: [255, 255, 255, 255],
                x: 0.5,
                y: 0.5,
                svg_content: String::new(),
            });
        }

        let mut open = self.show_title_editor;
        let mut save_title: Option<clipstrike_core::project::TitleAsset> = None;

        egui::Window::new("Title Editor")
            .open(&mut open)
            .resizable(true)
            .default_size([400.0, 300.0])
            .show(ctx, |ui| {
                if let Some(title) = &mut self.editing_title {
                    ui.horizontal(|ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut title.name);
                    });

                    ui.horizontal(|ui| {
                        ui.label("Template:");
                        egui::ComboBox::from_id_salt("title_template")
                            .selected_text(format!("{:?}", title.template))
                            .show_ui(ui, |ui| {
                                use clipstrike_core::project::TitleTemplate;
                                ui.selectable_value(&mut title.template, TitleTemplate::Centered, "Centered");
                                ui.selectable_value(&mut title.template, TitleTemplate::LowerThird, "Lower Third");
                                ui.selectable_value(&mut title.template, TitleTemplate::TopLeft, "Top Left");
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("Text:");
                        ui.text_edit_singleline(&mut title.text);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Font Size:");
                        ui.add(egui::Slider::new(&mut title.font_size, 8.0..=200.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Color:");
                        let mut color = egui::Color32::from_rgba_unmultiplied(
                            title.color[0],
                            title.color[1],
                            title.color[2],
                            title.color[3],
                        );
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            title.color = [color.r(), color.g(), color.b(), color.a()];
                        }
                    });

                    ui.collapsing("Manual Position", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Position X:");
                            ui.add(egui::Slider::new(&mut title.x, 0.0..=1.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Position Y:");
                            ui.add(egui::Slider::new(&mut title.y, 0.0..=1.0));
                        });
                    });

                    ui.separator();

                    if ui.button("Save Title Asset").clicked() {
                        use clipstrike_core::project::TitleTemplate;

                        let (final_x, final_y, anchor) = match title.template {
                            TitleTemplate::Centered => (0.5, 0.5, "middle"),
                            TitleTemplate::LowerThird => (0.5, 0.85, "middle"),
                            TitleTemplate::TopLeft => (0.1, 0.1, "start"),
                        };

                        let r = title.color[0];
                        let g = title.color[1];
                        let b = title.color[2];
                        let a = title.color[3] as f32 / 255.0;

                        title.svg_content = format!(
                            r#"<svg width="1920" height="1080" xmlns="http://www.w3.org/2000/svg">
                                <text x="{}" y="{}" font-family="{}" font-size="{}" fill="rgba({},{},{},{})" text-anchor="{}">{}</text>
                            </svg>"#,
                            final_x * 1920.0,
                            final_y * 1080.0,
                            title.font_family,
                            title.font_size,
                            r, g, b, a,
                            anchor,
                            title.text
                        );

                        save_title = Some(title.clone());
                    }
                }
            });

        if let Some(title) = save_title {
            self.project.titles.push(title);
            self.editing_title = None;
            self.show_title_editor = false;
        }
        self.show_title_editor = open;
    }
}

impl eframe::App for ClipStrikeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_arch = "wasm32")]
        {
            // Process incoming test commands
            for cmd in crate::test_hooks::pop_commands() {
                if cmd == "PLAY" {
                    self.is_playing = true;
                } else if cmd == "PAUSE" {
                    self.is_playing = false;
                } else if cmd == "ADD_TRACK" {
                    self.undo_stack.push(Box::new(clipstrike_core::commands::AddTrackCommand { track_id: None }), &mut self.project, &mut self.timeline);
                } else if cmd == "UNDO" {
                    self.undo_stack.undo(&mut self.project, &mut self.timeline);
                } else if cmd == "REDO" {
                    self.undo_stack.redo(&mut self.project, &mut self.timeline);
                } else if cmd == "ADD_TEST_ASSET" {
                    self.project.assets.push(clipstrike_core::project::MediaAsset {
                        id: 999,
                        name: "TestVideo.mp4".to_string(),
                        uri: "file:///test.mp4".to_string(),
                        duration_ms: 5000,
                        audio_levels: vec![],
                    });
                } else if cmd.starts_with("SEEK:") {
                    if let Ok(time_ms) = cmd["SEEK:".len()..].parse::<i64>() {
                        self.playhead_ms = time_ms;
                        self.seek_playhead();
                    }
                } else if cmd == "RAZOR_CLIP" {
                    if let Some(clip_id) = self.timeline.clips.first().map(|c| c.id) {
                        let playhead = self.playhead_ms;
                        self.undo_stack.push(
                            Box::new(clipstrike_core::commands::RazorClipCommand {
                                clip_id,
                                split_at: playhead,
                                original_clip: None,
                                new_clip_id: None,
                            }),
                            &mut self.project,
                            &mut self.timeline,
                        );
                    }
                } else if cmd == "TRIM_CLIP" {
                    if let Some(clip_id) = self.timeline.clips.first().map(|c| c.id) {
                        let _ = self.undo_stack.push(
                            Box::new(clipstrike_core::commands::TrimClipCommand {
                                clip_id,
                                old_start: 0,
                                old_end: 5000,
                                new_start: 1000,
                                new_end: 4000,
                            }),
                            &mut self.project,
                            &mut self.timeline,
                        );
                    }
                } else if cmd == "ADD_COMPLEX_ASSETS" {
                    self.project.assets.push(clipstrike_core::project::MediaAsset {
                        id: 1001,
                        name: "Forest.mp4".to_string(),
                        uri: "file:///forest.mp4".to_string(),
                        duration_ms: 5000,
                        audio_levels: vec![],
                    });
                    self.project.assets.push(clipstrike_core::project::MediaAsset {
                        id: 1002,
                        name: "Music.mp3".to_string(),
                        uri: "file:///music.mp3".to_string(),
                        duration_ms: 120000,
                        audio_levels: vec![],
                    });
                    self.project.assets.push(clipstrike_core::project::MediaAsset {
                        id: 1003,
                        name: "Logo.png".to_string(),
                        uri: "file:///logo.png".to_string(),
                        duration_ms: 0,
                        audio_levels: vec![],
                    });
                } else if cmd.starts_with("SEARCH_MEDIA:") {
                    self.asset_filter_text = cmd["SEARCH_MEDIA:".len()..].to_string();
                } else if cmd.starts_with("FILTER_TYPE:") {
                    if let Ok(idx) = cmd["FILTER_TYPE:".len()..].parse::<usize>() {
                        self.asset_filter_index = idx;
                    }
                } else if cmd.starts_with("SET_CLIP_PROP:") {
                    // Format: SET_CLIP_PROP:property_name:value
                    let parts: Vec<&str> = cmd["SET_CLIP_PROP:".len()..].split(':').collect();
                    if parts.len() == 2 {
                        let prop_name = parts[0];
                        if let Ok(value) = parts[1].parse::<f32>() {
                            if let Some(first_id) = self.timeline.clips.first().map(|c| c.id) {
                                let property = match prop_name {
                                    "PosX" => AnimatedProperty::PosX,
                                    "PosY" => AnimatedProperty::PosY,
                                    "ScaleX" => AnimatedProperty::ScaleX,
                                    "ScaleY" => AnimatedProperty::ScaleY,
                                    "Rotation" => AnimatedProperty::Rotation,
                                    "Opacity" => AnimatedProperty::Opacity,
                                    "Speed" => AnimatedProperty::Speed,
                                    _ => AnimatedProperty::Brightness,
                                };
                                if let Some(pos) = self.timeline.clips.iter_mut().position(|c| c.id == first_id) {
                                    let clip = &mut self.timeline.clips[pos];
                                    property_mut(clip, property).insert_keyframe(self.playhead_ms, value, clipstrike_core::keyframes::InterpolationMode::Linear);
                                }
                            }
                        }
                    }
                } else if cmd.starts_with("SET_CLIP_REVERSE:") {
                    let val = cmd["SET_CLIP_REVERSE:".len()..].parse::<bool>().unwrap_or(false);
                    if let Some(first_id) = self.timeline.clips.first().map(|c| c.id) {
                        if let Some(pos) = self.timeline.clips.iter_mut().position(|c| c.id == first_id) {
                            self.timeline.clips[pos].reverse = val;
                        }
                    }
                } else if cmd == "FREEZE_CLIP" {
                    if let Some(first_id) = self.timeline.clips.first().map(|c| c.id) {
                        if let Some(pos) = self.timeline.clips.iter_mut().position(|c| c.id == first_id) {
                            let current_source_ms = self.timeline.clips[pos].source_time_at(self.playhead_ms);
                            self.timeline.clips[pos].freeze_frame_ms = Some(current_source_ms);
                        }
                    }
                } else if cmd == "NEXT_FRAME" {
                    self.playhead_ms += self.frame_duration_ms();
                    self.seek_playhead();
                } else if cmd == "PREV_FRAME" {
                    self.playhead_ms = (self.playhead_ms - self.frame_duration_ms()).max(0);
                    self.seek_playhead();
                } else if cmd == "NEW" {
                    self.new_project();
                } else if cmd == "GET_PROJECT_JSON" {
                    self.project.capture_timeline(&self.timeline);
                    let project = self.snapshot_project();
                    if let Ok(json) = serde_json::to_string(&project) {
                        self.test_project_json_dump = Some(json);
                    }
                } else if cmd == "ADD_MARKER" {
                    self.toggle_marker_at_playhead();
                } else if cmd == "REMOVE_MARKER" {
                    self.toggle_marker_at_playhead();
                } else if cmd == "NEXT_MARKER" {
                    self.jump_to_next_marker();
                } else if cmd == "PREV_MARKER" {
                    self.jump_to_previous_marker();
                } else if cmd.starts_with("SET_THEME:") {
                    let theme = &cmd["SET_THEME:".len()..];
                    self.app_config.theme = match theme {
                        "Light" => ThemePreference::Light,
                        _ => ThemePreference::Dark,
                    };
                    self.apply_theme(ctx);
                    self.persist_app_config();
                } else if cmd.starts_with("SET_ADVANCED_VIEW:") {
                    let val = cmd["SET_ADVANCED_VIEW:".len()..].parse::<bool>().unwrap_or(true);
                    self.app_config.advanced_view = val;
                    self.persist_app_config();
                } else if cmd.starts_with("SET_FULLSCREEN:") {
                    let val = cmd["SET_FULLSCREEN:".len()..].parse::<bool>().unwrap_or(false);
                    self.app_config.fullscreen = val;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(val));
                    self.persist_app_config();
                } else if cmd.starts_with("ADD_CLIP_TO_TRACK:") {
                    let parts: Vec<&str> = cmd["ADD_CLIP_TO_TRACK:".len()..].split(':').collect();
                    if parts.len() == 2 {
                        if let (Ok(track_id), Ok(start_ms)) = (parts[0].parse::<usize>(), parts[1].parse::<i64>()) {
                            let clip = Clip {
                                id: rand::random::<usize>(),
                                asset_id: 999, // Use the test asset id
                                track_id,
                                blend_mode: clipstrike_core::timeline::BlendMode::Normal,
                                start: start_ms,
                                end: start_ms + 5000,
                                brightness: KeyframedProperty::new(1.0),
                                contrast: KeyframedProperty::new(1.0),
                                saturation: KeyframedProperty::new(1.0),
                                grayscale: KeyframedProperty::new(0.0),
                                blur_radius: KeyframedProperty::new(0.0),
                                volume: KeyframedProperty::new(1.0),
                                pos_x: KeyframedProperty::new(0.0),
                                pos_y: KeyframedProperty::new(0.0),
                                scale_x: KeyframedProperty::new(1.0),
                                scale_y: KeyframedProperty::new(1.0),
                                rotation: KeyframedProperty::new(0.0),
                                opacity: KeyframedProperty::new(1.0),
                                speed: KeyframedProperty::new(1.0),
                                reverse: false,
                                freeze_frame_ms: None,
                                audio_enabled: false,
                                video_enabled: true,
                                stabilization_enabled: false,
                                motion_tracking_enabled: false,
                                object_detection_enabled: false,
                                };                            self.undo_stack.push(Box::new(AddClipCommand { clip }), &mut self.project, &mut self.timeline);
                        }
                    }
                } else if cmd == "ADD_IMAGE_SEQUENCE" {
                    self.project.assets.push(clipstrike_core::project::MediaAsset {
                        id: 1004,
                        name: "Sequence_%04d.png".to_string(),
                        uri: "file:///seq_%04d.png".to_string(),
                        duration_ms: 10000,
                        audio_levels: vec![],
                    });
                } else if cmd.starts_with("SET_CLIP_STABILIZATION:") {
                    let val = cmd["SET_CLIP_STABILIZATION:".len()..].parse::<bool>().unwrap_or(false);
                    if let Some(first_id) = self.timeline.clips.first().map(|c| c.id) {
                        if let Some(pos) = self.timeline.clips.iter_mut().position(|c| c.id == first_id) {
                            self.timeline.clips[pos].stabilization_enabled = val;
                        }
                    }
                } else if cmd == "CREATE_TITLE" {
                    let title = clipstrike_core::project::TitleAsset {
                        id: rand::random::<usize>(),
                        name: "Test Title".to_string(),
                        template: clipstrike_core::project::TitleTemplate::default(),
                        text: "E2E Test".to_string(),
                        font_family: "Arial".to_string(),
                        font_size: 48.0,
                        color: [255, 255, 255, 255],
                        x: 0.5,
                        y: 0.5,
                        svg_content: r#"<svg width="1920" height="1080" xmlns="http://www.w3.org/2000/svg"><text x="960" y="540" font-family="Arial" font-size="48" fill="rgba(255,255,255,1)" text-anchor="middle">E2E Test</text></svg>"#.to_string(),
                    };
                    self.project.titles.push(title);
                } else if cmd == "DRAG_TITLE_TO_TIMELINE" {
                    if let Some(title) = self.project.titles.first() {
                        let title_id = title.id;
                        let track_id = self.project.tracks.first().map(|t| t.id).unwrap_or(0);
                        let clip = Clip {
                            id: rand::random::<usize>(),
                            asset_id: 0,
                            track_id,
                            blend_mode: clipstrike_core::timeline::BlendMode::Normal,
                            start: self.playhead_ms,                            end: self.playhead_ms + 5000,
                            brightness: KeyframedProperty::new(1.0),
                            contrast: KeyframedProperty::new(1.0),
                            saturation: KeyframedProperty::new(1.0),
                            grayscale: KeyframedProperty::new(0.0),
                            blur_radius: KeyframedProperty::new(0.0),
                            volume: KeyframedProperty::new(1.0),
                            pos_x: KeyframedProperty::new(0.0),
                            pos_y: KeyframedProperty::new(0.0),
                            scale_x: KeyframedProperty::new(1.0),
                            scale_y: KeyframedProperty::new(1.0),
                            rotation: KeyframedProperty::new(0.0),
                            opacity: KeyframedProperty::new(1.0),
                            speed: KeyframedProperty::new(1.0),
                            reverse: false,
                            freeze_frame_ms: None,
                            audio_enabled: false,
                            video_enabled: true,
                            stabilization_enabled: false,
                            motion_tracking_enabled: false,
                            object_detection_enabled: false,
                            };                        self.undo_stack.push(Box::new(AddClipCommand { clip }), &mut self.project, &mut self.timeline);
                    }
                } else if cmd.starts_with("SWITCH_TAB:") {
                    let tab_name = &cmd["SWITCH_TAB:".len()..];
                    self.left_panel_tab = match tab_name {
                        "Transitions" => LeftPanelTab::Transitions,
                        "Effects" => LeftPanelTab::Effects,
                        "Titles" => LeftPanelTab::Titles,
                        "Emojis" => LeftPanelTab::Emojis,
                        _ => LeftPanelTab::ProjectFiles,
                    };
                } else if cmd == "CLEAR_PROJECT_JSON" {
                    self.test_project_json_dump = None;
                } else if cmd.starts_with("LOAD_PROJECT_JSON:") {
                    let json = &cmd["LOAD_PROJECT_JSON:".len()..];
                    if let Ok(project) = serde_json::from_str::<clipstrike_core::project::Project>(json) {
                        self.reset_runtime_state(project, None);
                    }
                }
            }
            ctx.request_repaint();
        }

        // Task 7.2: OS Drag-and-drop import
        let dropped_paths: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        if !dropped_paths.is_empty() {
            self.import_media_paths(dropped_paths);
        }

        // Handle playback timing
        if self.is_playing {
            let dt = ctx.input(|i| i.stable_dt);
            self.playhead_ms += (dt * 1000.0) as i64;

            // Task 6.7: Looped playback
            if self.loop_enabled && self.playhead_ms >= self.loop_end_ms {
                self.playhead_ms = self.loop_start_ms;
                self.seek_playhead();
            }

            if let Some(audio) = &self.audio_engine {
                audio.set_playhead(self.playhead_ms);
            }

            ctx.request_repaint(); // Keep the loop running
        }

        // Sync engines with active clips at playhead
        let (all_active_clips, visible_active_clips) = self.active_clips_for_playhead();
        let frame_duration_ms = self.frame_duration_ms();
        let asset_duration_by_id: HashMap<usize, u64> = self
            .project
            .assets
            .iter()
            .map(|asset| (asset.id, asset.duration_ms))
            .collect();
        self.sync_engines_for_playhead(&all_active_clips, !self.is_playing);

        let mut request_new_project = false;
        let mut request_open_project = false;
        let mut request_save_project = false;
        let mut request_save_as_project = false;
        let mut request_open_recent_project: Option<PathBuf> = None;
        let mut request_toggle_fullscreen = false;

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project").clicked() {
                        request_new_project = true;
                    }
                    if ui.button("Open Project").clicked() {
                        request_open_project = true;
                    }
                    ui.menu_button("Recent Projects", |ui| {
                        if self.app_config.recent_projects.is_empty() {
                            ui.label("No recent projects");
                        } else {
                            for path in self.app_config.recent_projects.clone() {
                                let label = path.to_string_lossy();
                                if ui.button(label.as_ref()).clicked() {
                                    request_open_recent_project = Some(path);
                                    ui.close_menu();
                                }
                            }
                        }
                    });
                    ui.separator();
                    if ui.button("Save Project").clicked() {
                        request_save_project = true;
                    }
                    if ui.button("Save Project As...").clicked() {
                        request_save_as_project = true;
                    }
                    ui.separator();
                    if ui.button("Import Files...").clicked() {
                        if let Some(paths) = self.pick_media_files() {
                            self.import_media_paths(paths);
                        }
                    }
                    if ui.button("Choose Profile...").clicked() {
                        self.show_profile_dialog = true;
                    }
                    ui.separator();
                    if ui.button("Export Project").clicked() {}
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo").clicked() {
                        self.undo_stack.undo(&mut self.project, &mut self.timeline);
                    }
                    if ui.button("Redo").clicked() {
                        self.undo_stack.redo(&mut self.project, &mut self.timeline);
                    }
                    ui.separator();
                    if ui.button("Clear History").clicked() {
                        self.undo_stack = UndoStack::new();
                    }
                    if ui.button("Preferences").clicked() {
                        self.show_preferences_dialog = true;
                    }
                });
                ui.menu_button("Title", |ui| {
                    if ui.button("Title").clicked() {
                        self.show_title_editor = true;
                        ui.close_menu();
                    }
                    if ui.button("Animated Title").clicked() {}
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Fullscreen").clicked() {
                        request_toggle_fullscreen = true;
                    }
                    ui.menu_button("Views", |ui| {
                        if ui.button("Simple").clicked() {
                            self.app_config.advanced_view = false;
                            self.persist_app_config();
                        }
                        if ui.button("Advanced").clicked() {
                            self.app_config.advanced_view = true;
                            self.persist_app_config();
                        }
                    });
                });
                ui.menu_button("Help", |ui| if ui.button("About ClipStrike").clicked() {});
            });
        });

        egui::TopBottomPanel::top("main_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New").on_hover_text("New Project").clicked() {
                    request_new_project = true;
                }
                if ui.button("Open").on_hover_text("Open Project").clicked() {
                    request_open_project = true;
                }
                if ui.button("Save").on_hover_text("Save Project").clicked() {
                    request_save_project = true;
                }
                ui.separator();
                if ui.button("Undo").on_hover_text("Undo (Ctrl+Z)").clicked()
                    || (ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)))
                {
                    self.undo_stack.undo(&mut self.project, &mut self.timeline);
                }
                if ui.button("Redo").on_hover_text("Redo (Ctrl+Y)").clicked()
                    || (ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Y)))
                {
                    self.undo_stack.redo(&mut self.project, &mut self.timeline);
                }
                ui.separator();
                if ui.button("Import").on_hover_text("Import Files").clicked() {
                    if let Some(paths) = self.pick_media_files() {
                        self.import_media_paths(paths);
                    }
                }
                if ui
                    .button("Profile")
                    .on_hover_text("Choose Profile")
                    .clicked()
                {
                    self.show_profile_dialog = true;
                }
                if ui
                    .button("Fullscreen")
                    .on_hover_text("Fullscreen")
                    .clicked()
                {
                    request_toggle_fullscreen = true;
                }
                ui.separator();
                if ui.button("Export").on_hover_text("Export Video").clicked() {}
            });
        });

        if request_new_project {
            self.new_project();
        }
        if request_open_project {
            if let Err(err) = self.open_project_dialog() {
                log::error!("Failed to open project: {}", err);
            }
        }
        if let Some(path) = request_open_recent_project {
            if let Err(err) = self.open_project_at_path(path) {
                log::error!("Failed to open recent project: {}", err);
            }
        }
        if request_save_project {
            if let Err(err) = self.save_project() {
                log::error!("Failed to save project: {}", err);
            }
        }
        if request_save_as_project {
            if let Err(err) = self.save_project_as() {
                log::error!("Failed to save project as: {}", err);
            }
        }
        if request_toggle_fullscreen {
            self.app_config.fullscreen = !self.app_config.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                self.app_config.fullscreen,
            ));
            self.persist_app_config();
        }

        #[cfg(target_arch = "wasm32")]
        {
            let mut state_json = serde_json::json!({
                "playhead_ms": self.playhead_ms,
                "is_playing": self.is_playing,
                "tracks_count": self.project.tracks.len(),
                "clips_count": self.timeline.clips.len(),
                "transitions_count": self.timeline.transitions.len(),
                "assets_count": self.project.assets.len(),
                "markers_count": self.project.markers.len(),
                "left_panel_tab": format!("{:?}", self.left_panel_tab),
                "theme": format!("{:?}", self.app_config.theme),
                "advanced_view": self.app_config.advanced_view,
                "fullscreen": self.app_config.fullscreen,
                "test_project_json_dump": self.test_project_json_dump,
            });

            if let Some(clip) = self.timeline.clips.first() {
                state_json["test_first_clip_scale_x"] = clip.scale_x.get_value_at(self.playhead_ms).into();
                state_json["test_first_clip_brightness"] = clip.brightness.get_value_at(self.playhead_ms).into();
                state_json["test_first_clip_speed"] = clip.speed.get_value_at(self.playhead_ms).into();
                state_json["test_first_clip_opacity"] = clip.opacity.get_value_at(self.playhead_ms).into();
                state_json["test_first_clip_reverse"] = clip.reverse.into();
                state_json["test_first_clip_frozen"] = clip.freeze_frame_ms.is_some().into();
                state_json["test_first_clip_stabilization"] = clip.stabilization_enabled.into();
            }

            if let Some(rect) = self.test_first_track_rect {
                state_json["test_first_track_rect"] = serde_json::json!({
                    "left": rect.left(),
                    "top": rect.top(),
                    "width": rect.width(),
                    "height": rect.height(),
                });
            }
            if let Some(rect) = self.test_first_asset_rect {
                state_json["test_first_asset_rect"] = serde_json::json!({
                    "left": rect.left(),
                    "top": rect.top(),
                    "width": rect.width(),
                    "height": rect.height(),
                });
            }
            if let Some(rect) = self.test_first_clip_rect {
                state_json["test_first_clip_rect"] = serde_json::json!({
                    "left": rect.left(),
                    "top": rect.top(),
                    "width": rect.width(),
                    "height": rect.height(),
                });
            }
            if let Some(rect) = self.test_first_title_rect {
                state_json["test_first_title_rect"] = serde_json::json!({
                    "left": rect.left(),
                    "top": rect.top(),
                    "width": rect.width(),
                    "height": rect.height(),
                });
            }

            self.test_first_track_rect = None;
            self.test_first_asset_rect = None;
            self.test_first_clip_rect = None;
            self.test_first_title_rect = None;

            crate::test_hooks::set_state(state_json.to_string());
        }

        if self.show_profile_dialog {
            let mut open = self.show_profile_dialog;
            let mut close_after_apply = false;
            egui::Window::new("Choose Profile")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Current: {}x{} @ {:.2} fps",
                        self.project.width, self.project.height, self.project.framerate
                    ));
                    ui.separator();
                    for preset in ProjectProfile::presets() {
                        if ui
                            .button(format!(
                                "{}  [{} {}x{} @ {:.2} fps]",
                                preset.name,
                                preset.aspect_ratio_label(),
                                preset.width,
                                preset.height,
                                preset.framerate
                            ))
                            .clicked()
                        {
                            self.apply_profile_to_project(&preset);
                            close_after_apply = true;
                        }
                    }
                });
            self.show_profile_dialog = open && !close_after_apply;
        }

        if self.show_preferences_dialog {
            let mut open = self.show_preferences_dialog;
            let mut changed = false;
            egui::Window::new("Preferences")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Default Project Profile");
                    let profile = &mut self.app_config.default_project_profile;
                    changed |= ui.text_edit_singleline(&mut profile.name).changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut profile.width)
                                .range(16..=7680)
                                .speed(1),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut profile.height)
                                .range(16..=4320)
                                .speed(1),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut profile.framerate)
                                .range(1.0..=240.0)
                                .speed(1.0),
                        )
                        .changed();
                    ui.label(format!("Aspect Ratio: {}", profile.aspect_ratio_label()));
                    ui.separator();
                    changed |= ui
                        .checkbox(&mut self.app_config.autosave_enabled, "Enable autosave")
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(
                                &mut self.app_config.autosave_interval_minutes,
                                1..=30,
                            )
                            .text("Autosave interval (min)"),
                        )
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut self.app_config.advanced_view,
                            "Advanced view by default",
                        )
                        .changed();
                    ui.separator();
                    ui.label("Theme");
                    changed |= ui
                        .radio_value(&mut self.app_config.theme, ThemePreference::Dark, "Dark")
                        .changed();
                    changed |= ui
                        .radio_value(&mut self.app_config.theme, ThemePreference::Light, "Light")
                        .changed();
                });
            self.show_preferences_dialog = open;
            if changed {
                self.apply_theme(ctx);
                self.persist_app_config();
            }
        }

        // BOTTOM PANEL: Timeline
        egui::TopBottomPanel::bottom("timeline_panel")
            .resizable(true)
            .min_height(250.0)
            .show(ctx, |ui| {
                // Timeline Toolbar
                ui.horizontal(|ui| {
                    if ui.button("\u{2795} Track").on_hover_text("Add Track").clicked() {
                        if let Err(err) = self.apply_track_mutation(|project| {
                            project.add_track();
                            Ok(())
                        }) {
                            log::error!("Failed to add track: {}", err);
                        }
                    }
                    if ui.button("\u{1F9F2}").on_hover_text("Snapping Enabled").clicked() {}
                    ui.toggle_value(&mut self.razor_active, "\u{2702}")
                        .on_hover_text("Razor Tool");
                    if ui
                        .button("\u{1F4CC}")
                        .on_hover_text("Add or Remove Marker at Playhead")
                        .clicked()
                    {
                        self.toggle_marker_at_playhead();
                    }
                    if ui.button("\u{23EE}").on_hover_text("Previous Marker").clicked() {
                        self.jump_to_previous_marker();
                    }
                    if ui.button("\u{23ED}").on_hover_text("Next Marker").clicked() {
                        self.jump_to_next_marker();
                    }
                    if ui
                        .button("\u{1F3AF}")
                        .on_hover_text("Center Timeline on Playhead")
                        .clicked()
                    {}

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add(
                            egui::Slider::new(&mut self.zoom_level, 0.0..=1.0).show_value(false),
                        );
                        ui.label("\u{1F50D}");
                    });
                });
                ui.separator();

                // Timeline Tracks Area
                egui::ScrollArea::both().show(ui, |ui| {
                    let timeline_rect = ui.available_rect_before_wrap();

                    // Time ruler interaction
                    let (ruler_rect, ruler_res) = ui.allocate_exact_size(
                        egui::vec2(2000.0, 20.0),
                        egui::Sense::click_and_drag(),
                    );
                    if ruler_res.clicked() || ruler_res.dragged() {
                        if let Some(pos) = ruler_res.interact_pointer_pos() {
                            let local_x =
                                (pos.x - (ruler_rect.left() + TIMELINE_CONTENT_OFFSET_X)).max(0.0);
                            self.playhead_ms = (local_x / (self.zoom_level * 0.1)) as i64;

                            if ruler_res.clicked() {
                                let tolerance_px = 6.0;
                                if let Some(marker) = self.project.markers.iter().find(|marker| {
                                    let marker_x = ruler_rect.left()
                                        + (marker.time_ms as f32 * self.zoom_level * 0.1);
                                    (marker_x - pos.x).abs() <= tolerance_px
                                }) {
                                    self.playhead_ms = marker.time_ms;
                                    self.selected_marker_id = Some(marker.id);
                                } else {
                                    self.selected_marker_id = None;
                                }
                            }
                            self.seek_playhead();
                            if let Some(audio) = &self.audio_engine {
                                audio.set_playhead(self.playhead_ms);
                            }
                        }
                    }

                    ui.painter().rect_filled(
                        ruler_rect,
                        0.0,
                        ctx.style().visuals.widgets.noninteractive.bg_fill,
                    );
                    ui.painter().text(
                        egui::pos2(
                            ruler_rect.left() + TIMELINE_CONTENT_OFFSET_X + 8.0,
                            ruler_rect.center().y,
                        ),
                        egui::Align2::LEFT_CENTER,
                        "00:00:00:00                   00:00:05:00                   00:00:10:00",
                        egui::FontId::proportional(12.0),
                        ctx.style().visuals.text_color(),
                    );
                    let mut dragging_marker_id: Option<usize> = None;
                    let mut should_seek = false;
                    for marker in &mut self.project.markers {
                        let marker_x = ruler_rect.left()
                            + TIMELINE_CONTENT_OFFSET_X
                            + (marker.time_ms as f32 * self.zoom_level * 0.1);
                        let color = Self::marker_color(marker);
                        let stroke_width = if self.selected_marker_id == Some(marker.id) {
                            2.5
                        } else {
                            1.5
                        };
                        
                        let marker_handle_rect = egui::Rect::from_center_size(
                            egui::pos2(marker_x, ruler_rect.center().y),
                            egui::vec2(12.0, ruler_rect.height())
                        );
                        
                        let marker_res = ui.interact(
                            marker_handle_rect,
                            egui::Id::new(("marker", marker.id)),
                            egui::Sense::drag()
                        );
                        
                        if marker_res.dragged() {
                            if let Some(pos) = marker_res.interact_pointer_pos() {
                                let local_x = (pos.x - (ruler_rect.left() + TIMELINE_CONTENT_OFFSET_X)).max(0.0);
                                marker.time_ms = (local_x / (self.zoom_level * 0.1)) as i64;
                                self.playhead_ms = marker.time_ms;
                                should_seek = true;
                                dragging_marker_id = Some(marker.id);
                            }
                        }

                        if marker_res.clicked() {
                            self.selected_marker_id = Some(marker.id);
                            self.playhead_ms = marker.time_ms;
                            should_seek = true;
                        }

                        ui.painter().line_segment(
                            [
                                egui::pos2(marker_x, ruler_rect.top()),
                                egui::pos2(marker_x, ruler_rect.bottom()),
                            ],
                            egui::Stroke::new(stroke_width, color),
                        );
                        ui.painter().text(
                            egui::pos2(marker_x + 3.0, ruler_rect.top()),
                            egui::Align2::LEFT_TOP,
                            &marker.label,
                            egui::FontId::proportional(10.0),
                            color,
                        );
                    }
                    if should_seek {
                        self.seek_playhead();
                    }
                    if let Some(id) = dragging_marker_id {
                        self.selected_marker_id = Some(id);
                    }
                    ui.add_space(4.0);

                    // Draw Playhead line across all tracks
                    let playhead_x = ruler_rect.left()
                        + TIMELINE_CONTENT_OFFSET_X
                        + (self.playhead_ms as f32 * self.zoom_level * 0.1);
                    if playhead_x >= timeline_rect.left() && playhead_x <= timeline_rect.right() {
                        ui.painter().line_segment(
                            [
                                egui::pos2(playhead_x, timeline_rect.top()),
                                egui::pos2(playhead_x, timeline_rect.bottom()),
                            ],
                            egui::Stroke::new(2.0, egui::Color32::RED),
                        );
                    }

                    let mut razor_cmd = None;

                    let mut move_cmd = None;
                    let mut trim_cmd = None;
                    let mut add_clip_cmd = None;
                    let mut separate_audio_cmd = None;
                    let mut remove_clip_id = None;
                    let mut pending_track_reorder = None;
                    let mut pending_track_remove_id = None;
                    let mut pending_track_rename: Option<(usize, String)> = None;
                    let track_count = self.project.tracks.len();

                    // Tracks (OpenShot numbers tracks bottom-up by default)
                    for (track_order_index, track) in self.project.tracks.iter_mut().enumerate() {
                        let track_idx = track.id;
                        ui.horizontal(|ui| {
                            // Track Header
                            let header_response = ui
                                .allocate_ui(egui::vec2(TRACK_HEADER_WIDTH, 56.0), |ui| {
                                    ui.vertical(|ui| {
                                        if self.track_name_edit_id == Some(track.id) {
                                            let rename_response = ui.add(
                                                egui::TextEdit::singleline(
                                                    &mut self.track_name_buffer,
                                                )
                                                .desired_width(112.0),
                                            );
                                            let apply_clicked = ui.small_button("Apply").clicked();
                                            let cancel_clicked =
                                                ui.small_button("Cancel").clicked();
                                            let submit = rename_response.lost_focus()
                                                && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                            if apply_clicked || submit {
                                                pending_track_rename = Some((
                                                    track.id,
                                                    self.track_name_buffer.trim().to_string(),
                                                ));
                                                self.track_name_edit_id = None;
                                                self.track_name_buffer.clear();
                                            } else if cancel_clicked {
                                                self.track_name_edit_id = None;
                                                self.track_name_buffer.clear();
                                            }
                                        } else {
                                            let name_response = ui.label(&track.name);
                                            if name_response.double_clicked() {
                                                self.track_name_edit_id = Some(track.id);
                                                self.track_name_buffer = track.name.clone();
                                            }
                                        }

                                        ui.horizontal(|ui| {
                                            ui.toggle_value(&mut track.visible, "\u{1F441}")
                                                .on_hover_text("Toggle Visibility");
                                            ui.toggle_value(&mut track.audio_enabled, "\u{1F50A}")
                                                .on_hover_text("Mute / Unmute Track");
                                            ui.toggle_value(&mut track.solo, "S")
                                                .on_hover_text("Solo Track Audio");
                                            ui.toggle_value(&mut track.locked, "\u{1F512}")
                                                .on_hover_text("Lock Track");
                                            if ui
                                                .small_button("\u{2715}")
                                                .on_hover_text("Remove Track")
                                                .clicked()
                                            {
                                                pending_track_remove_id = Some(track.id);
                                            }
                                        });
                                        ui.small("Double-click to rename. Drag to reorder.");
                                    });
                                })
                                .response;
                            if header_response.double_clicked()
                                && self.track_name_edit_id != Some(track.id)
                            {
                                self.track_name_edit_id = Some(track.id);
                                self.track_name_buffer = track.name.clone();
                            }
                            if header_response.drag_stopped() {
                                let row_offset =
                                    (header_response.drag_delta().y / 58.0).round() as isize;
                                let target_index = (track_order_index as isize + row_offset)
                                    .clamp(0, track_count.saturating_sub(1) as isize)
                                    as usize;
                                if target_index != track_order_index {
                                    pending_track_reorder = Some((track_order_index, target_index));
                                }
                            }
                            ui.separator();

                            // Prevent interactions if locked
                            #[allow(deprecated)]
                            ui.set_enabled(!track.locked);
                            // Track Content
                            let (rect, _track_res) = ui.allocate_exact_size(
                                egui::vec2(2000.0, 50.0),
                                egui::Sense::hover(),
                            );

                            #[cfg(target_arch = "wasm32")]
                            {
                                if self.test_first_track_rect.is_none() {
                                    self.test_first_track_rect = Some(rect);
                                }
                            }

                            // Task 4.1: Drop target for media assets
                            if ui.rect_contains_pointer(rect)
                                && ctx.input(|i| i.pointer.any_released())
                            {
                                let dragging_id: Option<usize> =
                                    ctx.data(|d| d.get_temp(egui::Id::new("dragging_asset_id")));
                                let dragging_title_id: Option<usize> =
                                    ctx.data(|d| d.get_temp(egui::Id::new("dragging_title_id")));

                                if let Some(asset_id) = dragging_id {
                                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                                        let local_x = pos.x - rect.left();
                                        let start_ms = (local_x / (self.zoom_level * 0.1)) as i64;
                                        let end_ms = timeline_clip_end_ms(
                                            start_ms,
                                            asset_duration_by_id
                                                .get(&asset_id)
                                                .copied()
                                                .unwrap_or(0),
                                            frame_duration_ms,
                                        );

                                        // Create add command
                                        add_clip_cmd = Some(AddClipCommand {
                                            clip: Clip {
                                                id: rand::random::<usize>(),
                                                asset_id,
                                                track_id: track_idx,
                                                blend_mode: clipstrike_core::timeline::BlendMode::Normal,
                                                start: start_ms,
                                                end: end_ms,
                                                brightness: KeyframedProperty::new(1.0),
                                                contrast: KeyframedProperty::new(1.0),
                                                saturation: KeyframedProperty::new(1.0),
                                                grayscale: KeyframedProperty::new(0.0),
                                                blur_radius: KeyframedProperty::new(0.0),
                                                volume: KeyframedProperty::new(1.0),
                                                pos_x: KeyframedProperty::new(0.0),
                                                pos_y: KeyframedProperty::new(0.0),
                                                scale_x: KeyframedProperty::new(1.0),
                                                scale_y: KeyframedProperty::new(1.0),
                                                rotation: KeyframedProperty::new(0.0),
                                                opacity: KeyframedProperty::new(1.0),
                                                speed: KeyframedProperty::new(1.0),
                                                reverse: false,
                                                freeze_frame_ms: None,
                                                audio_enabled: true,
                                                video_enabled: true,
                                                stabilization_enabled: false,
                                                motion_tracking_enabled: false,
                                                object_detection_enabled: false,
                                            },
                                        });
                                        // Clear dragging state
                                        ctx.data_mut(|d| {
                                            d.remove_temp::<usize>(egui::Id::new(
                                                "dragging_asset_id",
                                            ))
                                        });
                                    }
                                } else if let Some(title_id) = dragging_title_id {
                                    if let Some(title) = self.project.titles.iter().find(|t| t.id == title_id) {
                                        if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                                            let local_x = pos.x - rect.left();
                                            let start_ms = (local_x / (self.zoom_level * 0.1)) as i64;
                                            
                                            // Task 13.2: Convert title to a temporary asset
                                            #[cfg(not(target_arch = "wasm32"))]
                                            let uri = {
                                                let temp_dir = std::env::temp_dir();
                                                let svg_path = temp_dir.join(format!("title_{}.svg", title.id));
                                                let _ = std::fs::write(&svg_path, &title.svg_content);
                                                format!("file:///{}", svg_path.to_string_lossy().replace("\\", "/"))
                                            };

                                            #[cfg(target_arch = "wasm32")]
                                            let uri = {
                                                use base64::{Engine as _, engine::general_purpose};
                                                let b64 = general_purpose::STANDARD.encode(&title.svg_content);
                                                format!("data:image/svg+xml;base64,{}", b64)
                                            };
                                            
                                            let asset_id = rand::random::<usize>();
                                            
                                            self.project.assets.push(MediaAsset {
                                                id: asset_id,
                                                name: title.name.clone(),
                                                uri,
                                                duration_ms: 10000,
                                                audio_levels: Vec::new(),
                                            });

                                            add_clip_cmd = Some(AddClipCommand {
                                                clip: Clip {
                                                    id: rand::random::<usize>(),
                                                    asset_id,
                                                    track_id: track_idx,
                                                    blend_mode: clipstrike_core::timeline::BlendMode::Normal,
                                                    start: start_ms,
                                                    end: start_ms + 5000,
                                                    brightness: KeyframedProperty::new(1.0),
                                                    contrast: KeyframedProperty::new(1.0),
                                                    saturation: KeyframedProperty::new(1.0),
                                                    grayscale: KeyframedProperty::new(0.0),
                                                    blur_radius: KeyframedProperty::new(0.0),
                                                    volume: KeyframedProperty::new(1.0),
                                                    pos_x: KeyframedProperty::new(0.0),
                                                    pos_y: KeyframedProperty::new(0.0),
                                                    scale_x: KeyframedProperty::new(1.0),
                                                    scale_y: KeyframedProperty::new(1.0),
                                                    rotation: KeyframedProperty::new(0.0),
                                                    opacity: KeyframedProperty::new(1.0),
                                                    speed: KeyframedProperty::new(1.0),
                                                    reverse: false,
                                                    freeze_frame_ms: None,
                                                    audio_enabled: false,
                                                    video_enabled: true,
                                                    stabilization_enabled: false,
                                                    motion_tracking_enabled: false,
                                                    object_detection_enabled: false,
                                                },
                                            });
                                            ctx.data_mut(|d| {
                                                d.remove_temp::<usize>(egui::Id::new(
                                                    "dragging_title_id",
                                                ))
                                            });
                                        }
                                    }
                                }
                            }

                            // Background
                            ui.painter().rect_filled(
                                rect,
                                2.0,
                                ctx.style()
                                    .visuals
                                    .widgets
                                    .noninteractive
                                    .bg_fill
                                    .linear_multiply(0.3),
                            );

                            // Task 3.2: Timeline Virtualization
                            // Calculate visible time range based on scroll position and viewport width
                            let view_start_ms = (ui.clip_rect().left() - rect.left()).max(0.0)
                                / (self.zoom_level * 0.1);
                            let view_end_ms =
                                (ui.clip_rect().right() - rect.left()) / (self.zoom_level * 0.1);

                            let visible_clips = self
                                .timeline
                                .get_visible_clips(view_start_ms as i64, view_end_ms as i64);

                            // Draw only visible clips for this track
                            let mut drew_transitions_for_track = false;
                            for visible_clip in visible_clips {
                                if visible_clip.track_id == track_idx {
                                    if let Some(clip_index) = self
                                        .timeline
                                        .clips
                                        .iter()
                                        .position(|c| c.id == visible_clip.id)
                                    {
                                        let clip = self.timeline.clips[clip_index].clone();
                                        let pixels_per_ms = (self.zoom_level * 0.1).max(0.001);
                                        let is_active_drag = self.drag_clip_id == Some(clip.id);
                                        let (display_start, display_end) = if is_active_drag {
                                            self.drag_preview_times.unwrap_or((clip.start, clip.end))
                                        } else {
                                            (clip.start, clip.end)
                                        };
                                        let x_start =
                                            rect.left() + (display_start as f32 * pixels_per_ms);
                                        let x_end = rect.left() + (display_end as f32 * pixels_per_ms);

                                        let clip_rect = egui::Rect::from_min_max(
                                            egui::pos2(x_start, rect.top() + 5.0),
                                            egui::pos2(x_end, rect.bottom() - 5.0),
                                        );

                                        #[cfg(target_arch = "wasm32")]
                                        {
                                            if self.test_first_clip_rect.is_none() {
                                                self.test_first_clip_rect = Some(clip_rect);
                                            }
                                        }

                                        if clip_rect.min.x < rect.right()
                                            && clip_rect.max.x > rect.left()
                                        {
                                            let paint_rect = clip_rect.intersect(rect);
                                            let clip_id = egui::Id::new(clip.id);
                                            let (asset_name, audio_levels) = self
                                                .project
                                                .assets
                                                .iter()
                                                .find(|asset| asset.id == clip.asset_id)
                                                .map(|asset| {
                                                    (asset.name.clone(), asset.audio_levels.clone())
                                                })
                                                .unwrap_or_else(|| {
                                                    ("Unknown Media".to_string(), Vec::new())
                                                });
                                            let clip_label = timeline_clip_label(
                                                &asset_name,
                                                clip.id,
                                                ((paint_rect.width() / 7.0).floor() as usize)
                                                    .max(8),
                                            );
                                            let clip_tooltip =
                                                timeline_clip_tooltip(&asset_name, clip.id);

                                            let left_handle_rect = egui::Rect::from_min_max(
                                                egui::pos2(paint_rect.left(), paint_rect.top()),
                                                egui::pos2(
                                                    paint_rect.left() + 10.0,
                                                    paint_rect.bottom(),
                                                ),
                                            );
                                            let right_handle_rect = egui::Rect::from_min_max(
                                                egui::pos2(
                                                    paint_rect.right() - 10.0,
                                                    paint_rect.top(),
                                                ),
                                                egui::pos2(paint_rect.right(), paint_rect.bottom()),
                                            );

                                            let left_res = ui.interact(
                                                left_handle_rect,
                                                clip_id.with("left"),
                                                egui::Sense::drag(),
                                            );
                                            let right_res = ui.interact(
                                                right_handle_rect,
                                                clip_id.with("right"),
                                                egui::Sense::drag(),
                                            );
                                            let body_res = ui
                                                .interact(paint_rect, clip_id, egui::Sense::drag())
                                                .on_hover_text(clip_tooltip.clone());
                                            if left_res.hovered() || right_res.hovered() {
                                                ctx.set_cursor_icon(
                                                    egui::CursorIcon::ResizeHorizontal,
                                                );
                                            }

                                            let started_drag = left_res.drag_started()
                                                || right_res.drag_started()
                                                || body_res.drag_started();
                                            if started_drag
                                                && (self.drag_clip_id.is_none()
                                                    || self.drag_clip_id == Some(clip.id))
                                            {
                                                self.drag_start_times =
                                                    Some((clip.start, clip.end));
                                                self.drag_pointer_start_x = ctx
                                                    .input(|input| input.pointer.interact_pos())
                                                    .map(|pos| pos.x)
                                                    .or_else(|| {
                                                        left_res
                                                            .interact_pointer_pos()
                                                            .or_else(|| right_res.interact_pointer_pos())
                                                            .or_else(|| body_res.interact_pointer_pos())
                                                            .map(|pos| pos.x)
                                                    });
                                                self.drag_clip_id = Some(clip.id);
                                                self.drag_preview_times = Some((clip.start, clip.end));
                                                if left_res.drag_started() {
                                                    self.trim_side = Some(TrimSide::Left);
                                                } else if right_res.drag_started() {
                                                    self.trim_side = Some(TrimSide::Right);
                                                } else {
                                                    self.trim_side = None;
                                                }
                                            }

                                            // Drag preview/commit is handled centrally after track drawing.
                                            if body_res.clicked() {
                                                if self.razor_active {
                                                    if let Some(pos) =
                                                        body_res.interact_pointer_pos()
                                                    {
                                                        let local_x = pos.x - rect.left();
                                                        let split_at =
                                                            (local_x / pixels_per_ms) as i64;
                                                        razor_cmd = Some((clip.id, split_at));
                                                    }
                                                } else {
                                                    self.selected_clip_id = Some(clip.id);
                                                }
                                            }

                                            body_res.context_menu(|ui| {
                                                if ui.button("\u{2702} Slice").clicked() {
                                                    razor_cmd = Some((clip.id, self.playhead_ms));
                                                    ui.close_menu();
                                                }
                                                if ui.button("\u{1F50A} Separate Audio").clicked() {
                                                    let mut audio_clip = clip.clone();
                                                    audio_clip.id = rand::random::<usize>();
                                                    audio_clip.track_id += 1; // Move to next track
                                                    audio_clip.video_enabled = false;
                                                    separate_audio_cmd = Some(SeparateAudioCommand {
                                                        clip_id: clip.id,
                                                        audio_clip,
                                                    });
                                                    ui.close_menu();
                                                }
                                                ui.separator();
                                                if ui.button("\u{1F5D1} Remove").clicked() {
                                                    remove_clip_id = Some(clip.id);
                                                    ui.close_menu();
                                                }
                                            });

                                            let is_selected =
                                                self.selected_clip_id == Some(clip.id);
                                            let is_dragging = is_active_drag
                                                || left_res.dragged()
                                                || right_res.dragged()
                                                || body_res.dragged();
                                            let color = if is_selected {
                                                egui::Color32::from_rgb(255, 170, 0)
                                            } else if is_dragging {
                                                egui::Color32::from_rgb(100, 160, 255)
                                            } else {
                                                egui::Color32::from_rgb(60, 120, 200)
                                            };

                                            ui.painter().rect_filled(paint_rect, 4.0, color);
                                            if !audio_levels.is_empty() && paint_rect.width() > 24.0
                                            {
                                                let waveform_rect = egui::Rect::from_min_max(
                                                    egui::pos2(
                                                        paint_rect.left() + 8.0,
                                                        paint_rect.center().y,
                                                    ),
                                                    egui::pos2(
                                                        paint_rect.right() - 8.0,
                                                        paint_rect.bottom() - 6.0,
                                                    ),
                                                );
                                                let sample_count = ((waveform_rect.width() / 3.0)
                                                    .floor()
                                                    as usize)
                                                    .clamp(8, audio_levels.len().max(8));
                                                let denominator =
                                                    sample_count.saturating_sub(1).max(1);
                                                let mid_y = waveform_rect.center().y;
                                                for sample in 0..sample_count {
                                                    let src_index = if audio_levels.len() <= 1 {
                                                        0
                                                    } else {
                                                        sample
                                                            .saturating_mul(audio_levels.len() - 1)
                                                            / denominator
                                                    };
                                                    let amplitude = audio_levels
                                                        .get(src_index)
                                                        .copied()
                                                        .unwrap_or(0.0)
                                                        .clamp(0.0, 1.0);
                                                    let x = egui::lerp(
                                                        waveform_rect.left()
                                                            ..=waveform_rect.right(),
                                                        sample as f32 / denominator as f32,
                                                    );
                                                    let half_height =
                                                        (amplitude * waveform_rect.height() * 0.45)
                                                            .max(1.0);
                                                    ui.painter().line_segment(
                                                        [
                                                            egui::pos2(x, mid_y - half_height),
                                                            egui::pos2(x, mid_y + half_height),
                                                        ],
                                                        egui::Stroke::new(
                                                            1.0,
                                                            egui::Color32::from_white_alpha(150),
                                                        ),
                                                    );
                                                }
                                            }
                                            ui.painter().text(
                                                egui::pos2(
                                                    paint_rect.center().x,
                                                    paint_rect.top() + 14.0,
                                                ),
                                                egui::Align2::CENTER_CENTER,
                                                clip_label.clone(),
                                                egui::FontId::proportional(12.0),
                                                egui::Color32::WHITE,
                                            );
                                        }
                                    }
                                }

                                if !drew_transitions_for_track {
                                    drew_transitions_for_track = true;
                                    let dragging_transition: Option<DraggedTransition> =
                                        ctx.data(|data| {
                                            data.get_temp(egui::Id::new("dragging_transition"))
                                        });
                                    for trans in &mut self.timeline.transitions {
                                        if trans.track_id != track_idx {
                                            continue;
                                        }
                                        let x_start = rect.left()
                                            + (trans.start_ms as f32 * self.zoom_level * 0.1);
                                        let x_end = rect.left()
                                            + (trans.end_ms as f32 * self.zoom_level * 0.1);
                                        let trans_rect = egui::Rect::from_min_max(
                                            egui::pos2(x_start, rect.top() + 2.0),
                                            egui::pos2(x_end, rect.bottom() - 2.0),
                                        );
                                        let trans_response = ui.interact(
                                            trans_rect,
                                            egui::Id::new(("transition_drop", trans.id)),
                                            egui::Sense::click(),
                                        );
                                        if trans_response.hovered() {
                                            ctx.set_cursor_icon(egui::CursorIcon::Grab);
                                        }
                                        if trans_response.hovered()
                                            && ctx.input(|input| input.pointer.any_released())
                                        {
                                            if let Some(payload) = dragging_transition {
                                                trans.transition_type = payload.transition_type;
                                                trans.custom_mask_id = payload.custom_mask_id;
                                                if trans.transition_type
                                                    != TransitionType::CustomMask
                                                {
                                                    trans.custom_mask_id = None;
                                                }
                                                ctx.data_mut(|data| {
                                                    data.remove_temp::<DraggedTransition>(
                                                        egui::Id::new("dragging_transition"),
                                                    )
                                                });
                                            }
                                        }
                                        let color = if dragging_transition.is_some()
                                            && trans_response.hovered()
                                        {
                                            egui::Color32::from_rgba_unmultiplied(255, 200, 0, 160)
                                        } else {
                                            egui::Color32::from_rgba_unmultiplied(255, 255, 0, 100)
                                        };
                                        ui.painter().rect_filled(trans_rect, 4.0, color);
                                        ui.painter().text(
                                            trans_rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            trans.transition_type.label(),
                                            egui::FontId::proportional(11.0),
                                            egui::Color32::BLACK,
                                        );
                                    }
                                }
                            }
                        });
                        ui.add_space(2.0);
                    }

                    if let Some((track_id, new_name)) = pending_track_rename.take() {
                        if let Err(err) = self.apply_track_mutation(|project| {
                            project.rename_track(track_id, &new_name)
                        }) {
                            log::error!("Failed to rename track: {}", err);
                        }
                    }

                    if let Some(track_id) = pending_track_remove_id.take() {
                        if let Err(err) = self.apply_track_mutation(|project| {
                            project.remove_track(track_id).map(|_| ())
                        }) {
                            log::error!("Failed to remove track: {}", err);
                        }
                    }

                    if let Some((from_index, to_index)) = pending_track_reorder.take() {
                        if let Err(err) = self.apply_track_mutation(|project| {
                            project.reorder_tracks(from_index, to_index)
                        }) {
                            log::error!("Failed to reorder tracks: {}", err);
                        }
                    }

                    let pointer_is_down = ctx.input(|input| input.pointer.primary_down());
                    if let Some(active_drag_clip_id) = self.drag_clip_id {
                        if pointer_is_down {
                            if let (
                                Some((old_start, old_end)),
                                Some(pointer_start_x),
                                Some(current_pointer_pos),
                            ) = (
                                self.drag_start_times,
                                self.drag_pointer_start_x,
                                ctx.input(|input| input.pointer.latest_pos()),
                            ) {
                                let pixels_per_ms = (self.zoom_level * 0.1).max(0.001);
                                let snap_radius_ms = ((12.0 / pixels_per_ms).round() as i64).max(1);
                                let delta_x = current_pointer_pos.x - pointer_start_x;
                                let delta_ms = (delta_x / pixels_per_ms) as i64;
                                let (new_start, new_end) = if let Some(side) = self.trim_side {
                                    self.timeline.preview_trim(
                                        old_start,
                                        old_end,
                                        side == TrimSide::Left,
                                        delta_ms,
                                        snap_radius_ms,
                                        &[old_start, old_end],
                                    )
                                } else {
                                    self.timeline.preview_move(
                                        old_start,
                                        old_end,
                                        delta_ms,
                                        snap_radius_ms,
                                        &[old_start, old_end],
                                    )
                                };
                                self.drag_preview_times = Some((new_start, new_end));
                            }
                        } else {
                            if let (Some((old_start, old_end)), Some((new_start, new_end))) =
                                (self.drag_start_times, self.drag_preview_times)
                            {
                                if self.trim_side.is_some() {
                                    trim_cmd = Some(TrimClipCommand {
                                        clip_id: active_drag_clip_id,
                                        old_start,
                                        old_end,
                                        new_start,
                                        new_end,
                                    });
                                } else {
                                    move_cmd = Some(MoveClipCommand {
                                        clip_id: active_drag_clip_id,
                                        old_start,
                                        old_end,
                                        new_start,
                                        new_end,
                                    });
                                }
                            }
                            self.drag_start_times = None;
                            self.drag_pointer_start_x = None;
                            self.drag_clip_id = None;
                            self.drag_preview_times = None;
                            self.trim_side = None;
                        }
                    }

                    if let Some((id, split_at)) = razor_cmd {
                        let _ = self.timeline.razor(id, split_at);
                    }

                    if let Some(cmd) = move_cmd {
                        self.undo_stack.push(Box::new(cmd), &mut self.project, &mut self.timeline);
                    }

                    if let Some(cmd) = trim_cmd {
                        self.undo_stack.push(Box::new(cmd), &mut self.project, &mut self.timeline);
                    }

                    if let Some(cmd) = add_clip_cmd {
                        self.undo_stack.push(Box::new(cmd), &mut self.project, &mut self.timeline);
                    }

                    if let Some(cmd) = separate_audio_cmd {
                        self.undo_stack.push(Box::new(cmd), &mut self.project, &mut self.timeline);
                    }

                    if let Some(clip_id) = remove_clip_id {
                        if let Some(pos) = self.timeline.clips.iter().position(|c| c.id == clip_id) {
                            let clip = self.timeline.clips[pos].clone();
                            self.undo_stack.push(Box::new(RemoveClipCommand { clip }), &mut self.project, &mut self.timeline);
                        }
                    }
                });
            });

        // LEFT PANEL: Project Files / Transitions / Effects / Emojis
        if self.app_config.advanced_view {
            egui::SidePanel::left("left_dock")
                .resizable(true)
                .min_width(300.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.left_panel_tab,
                            LeftPanelTab::ProjectFiles,
                            "Project Files",
                        );
                        ui.selectable_value(
                            &mut self.left_panel_tab,
                            LeftPanelTab::Transitions,
                            "Transitions",
                        );
                        ui.selectable_value(
                            &mut self.left_panel_tab,
                            LeftPanelTab::Effects,
                            "Effects",
                        );
                        ui.selectable_value(
                            &mut self.left_panel_tab,
                            LeftPanelTab::Emojis,
                            "Emojis",
                        );
                    });
                    ui.separator();

                    match self.left_panel_tab {
                        LeftPanelTab::ProjectFiles => {
                            ui.horizontal(|ui| {
                                if ui.button("+").clicked() {
                                    let start = self.timeline.clips.len() as i64 * 1000;
                                    self.timeline.add_clip(Clip {
                                        id: rand::random::<usize>(),
                                        asset_id: 0,
                                        track_id: 5,
                                        blend_mode: clipstrike_core::timeline::BlendMode::Normal,
                                        start,
                                        end: start + 2000,
                                        brightness: KeyframedProperty::new(1.0),
                                        contrast: KeyframedProperty::new(1.0),
                                        saturation: KeyframedProperty::new(1.0),
                                        grayscale: KeyframedProperty::new(0.0),
                                        blur_radius: KeyframedProperty::new(0.0),
                                        volume: KeyframedProperty::new(1.0),
                                        pos_x: KeyframedProperty::new(0.0),
                                        pos_y: KeyframedProperty::new(0.0),
                                        scale_x: KeyframedProperty::new(1.0),
                                        scale_y: KeyframedProperty::new(1.0),
                                        rotation: KeyframedProperty::new(0.0),
                                        opacity: KeyframedProperty::new(1.0),
                                        speed: KeyframedProperty::new(1.0),
                                        reverse: false,
                                        freeze_frame_ms: None,
                                        audio_enabled: false,
                                        video_enabled: true,
                                        stabilization_enabled: false,
                                        motion_tracking_enabled: false,
                                        object_detection_enabled: false,
                                    });
                                }
                                let _ = ui.button("-");
                                ui.label("Show:");

                                let filter_labels = ["All", "Video", "Audio", "Image"];
                                egui::ComboBox::from_id_salt("show_filter")
                                    .selected_text(filter_labels[self.asset_filter_index])
                                    .show_ui(ui, |ui| {
                                        for (i, label) in filter_labels.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut self.asset_filter_index,
                                                i,
                                                *label,
                                            );
                                        }
                                    });

                                ui.text_edit_singleline(&mut self.asset_filter_text)
                                    .on_hover_text("Filter by name");
                            });
                            ui.separator();
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                if self.project.assets.is_empty() {
                                    ui.label("Drop Files Here...");
                                } else {
                                    let asset_ids: Vec<usize> = self.project.assets.iter()
                                        .filter(|asset| {
                                            if !self.asset_filter_text.is_empty()
                                                && !asset
                                                    .name
                                                    .to_lowercase()
                                                    .contains(&self.asset_filter_text.to_lowercase())
                                            {
                                                return false;
                                            }

                                            let ext = asset
                                                .uri
                                                .split('.')
                                                .last()
                                                .unwrap_or("")
                                                .to_lowercase();
                                            match self.asset_filter_index {
                                                0 => true,
                                                1 => {
                                                    ["mp4", "mkv", "mov", "avi"].contains(&ext.as_str())
                                                }
                                                2 => ["wav", "mp3", "flac"].contains(&ext.as_str()),
                                                3 => ["png", "jpg", "jpeg", "bmp"]
                                                    .contains(&ext.as_str()),
                                                _ => true,
                                            }
                                        })
                                        .map(|a| a.id)
                                        .collect();

                                    for id in asset_ids {
                                        let asset_data = {
                                            let asset = self.project.assets.iter().find(|a| a.id == id).unwrap();
                                            (asset.id, asset.name.clone(), asset.uri.clone(), asset.duration_ms)
                                        };
                                        let (asset_id, asset_name, asset_uri, asset_duration_ms) = asset_data;

                                        let ext = asset_uri.split('.').last().unwrap_or("").to_lowercase();
                                        let item_id = egui::Id::new(("asset", asset_id));
                                        
                                        let mut add_to_timeline = false;
                                        ui.horizontal(|ui| {
                                            let icon =
                                                if ["png", "jpg", "jpeg"].contains(&ext.as_str()) {
                                                    "Image"
                                                } else {
                                                    "Media"
                                                };
                                            ui.label(icon);
                                            let label_res = ui.selectable_label(false, &asset_name);

                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                if self.test_first_asset_rect.is_none() {
                                                    self.test_first_asset_rect = Some(label_res.rect);
                                                }
                                            }

                                            if ui
                                                .interact(
                                                    label_res.rect,
                                                    item_id,
                                                    egui::Sense::drag(),
                                                )
                                                .dragged()
                                            {
                                                ctx.data_mut(|d| {
                                                    d.insert_temp(
                                                        egui::Id::new("dragging_asset_id"),
                                                        asset_id,
                                                    )
                                                });
                                            }

                                            if ui
                                                .button("+")
                                                .on_hover_text("Add to Timeline")
                                                .clicked()
                                            {
                                                add_to_timeline = true;
                                            }
                                        });

                                        if add_to_timeline {
                                            let end_ms = timeline_clip_end_ms(
                                                self.playhead_ms,
                                                asset_duration_ms,
                                                frame_duration_ms,
                                            );
                                            let clip = Clip {
                                                id: rand::random::<usize>(),
                                                asset_id,
                                                track_id: 5,
                                                blend_mode: clipstrike_core::timeline::BlendMode::Normal,
                                                start: self.playhead_ms,
                                                end: end_ms,
                                                brightness: KeyframedProperty::new(1.0),
                                                contrast: KeyframedProperty::new(1.0),
                                                saturation: KeyframedProperty::new(1.0),
                                                grayscale: KeyframedProperty::new(0.0),
                                                blur_radius: KeyframedProperty::new(0.0),
                                                volume: KeyframedProperty::new(1.0),
                                                pos_x: KeyframedProperty::new(0.0),
                                                pos_y: KeyframedProperty::new(0.0),
                                                scale_x: KeyframedProperty::new(1.0),
                                                scale_y: KeyframedProperty::new(1.0),
                                                rotation: KeyframedProperty::new(0.0),
                                                opacity: KeyframedProperty::new(1.0),
                                                speed: KeyframedProperty::new(1.0),
                                                reverse: false,
                                                freeze_frame_ms: None,
                                                audio_enabled: false,
                                                video_enabled: true,
                                                stabilization_enabled: false,
                                                motion_tracking_enabled: false,
                                                object_detection_enabled: false,
                                                };
                                            self.undo_stack.push(
                                                Box::new(AddClipCommand { clip }),
                                                &mut self.project,
                                                &mut self.timeline,
                                            );
                                        }
                                    }
                                }
                            });
                        }
                        LeftPanelTab::Transitions => {
                            ui.label("Transitions library");
                            ui.separator();
                            if ui.button("Import Luma Mask").clicked() {
                                if let Some(path) = self.pick_luma_mask_file() {
                                    match image::open(&path) {
                                        Ok(image) => {
                                            let mask = image.to_luma8();
                                            let dimensions = mask.dimensions();
                                            let name = path
                                                .file_stem()
                                                .and_then(|stem| stem.to_str())
                                                .unwrap_or("Imported Mask")
                                                .to_string();
                                            self.project.transition_masks.push(
                                                TransitionMaskAsset {
                                                    id: rand::random::<usize>(),
                                                    name,
                                                    width: dimensions.0,
                                                    height: dimensions.1,
                                                    pixels: mask.into_raw(),
                                                },
                                            );
                                        }
                                        Err(err) => {
                                            log::error!("Failed to import luma mask: {}", err);
                                        }
                                    }
                                }
                            }
                            ui.separator();

                            for transition_type in [
                                TransitionType::Crossfade,
                                TransitionType::WipeLeft,
                                TransitionType::WipeRight,
                                TransitionType::WipeUp,
                                TransitionType::WipeDown,
                                TransitionType::WipeDiagonal,
                                TransitionType::WipeClock,
                            ] {
                                let drag_id =
                                    egui::Id::new(("drag_transition", transition_type.label()));
                                let payload = DraggedTransition {
                                    transition_type,
                                    custom_mask_id: None,
                                };
                                let response = ui.selectable_label(false, transition_type.label());
                                if ui
                                    .interact(response.rect, drag_id, egui::Sense::drag())
                                    .dragged()
                                {
                                    ctx.data_mut(|data| {
                                        data.insert_temp(
                                            egui::Id::new("dragging_transition"),
                                            payload,
                                        )
                                    });
                                }
                            }

                            if !self.project.transition_masks.is_empty() {
                                ui.separator();
                                ui.label("Custom Masks");
                                for mask in &self.project.transition_masks {
                                    let response =
                                        ui.selectable_label(false, format!("Mask: {}", mask.name));
                                    let payload = DraggedTransition {
                                        transition_type: TransitionType::CustomMask,
                                        custom_mask_id: Some(mask.id),
                                    };
                                    if ui
                                        .interact(
                                            response.rect,
                                            egui::Id::new(("drag_transition_mask", mask.id)),
                                            egui::Sense::drag(),
                                        )
                                        .dragged()
                                    {
                                        ctx.data_mut(|data| {
                                            data.insert_temp(
                                                egui::Id::new("dragging_transition"),
                                                payload,
                                            )
                                        });
                                    }
                                }
                            }
                        }
                        LeftPanelTab::Effects => {
                            ui.label("Effects library");
                            ui.separator();
                            ui.label("Brightness");
                            ui.label("Contrast");
                            ui.label("Saturation");
                            ui.label("Grayscale");
                            ui.label("Blur");
                        }
                        LeftPanelTab::Titles => {
                            ui.label("Custom Titles");
                            ui.separator();
                            if ui.button("\u{2795} Create New Title").clicked() {
                                self.show_title_editor = true;
                            }
                            ui.separator();
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                if self.project.titles.is_empty() {
                                    ui.label("No titles created yet.");
                                } else {
                                    let title_ids: Vec<usize> = self.project.titles.iter().map(|t| t.id).collect();
                                    for id in title_ids {
                                        let title_data = {
                                            let title = self.project.titles.iter().find(|t| t.id == id).unwrap();
                                            (title.id, title.name.clone())
                                        };
                                        let (title_id, title_name) = title_data;

                                        let mut add_to_timeline = false;
                                        ui.horizontal(|ui| {
                                            ui.label("\u{1F3F7}");
                                            let title_label_res = ui.label(&title_name);
                                            
                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                if self.test_first_title_rect.is_none() {
                                                    self.test_first_title_rect = Some(title_label_res.rect);
                                                }
                                            }
                                            
                                            // Task 13.2: Drag source for titles
                                            let item_id = egui::Id::new(("title_asset", title_id));
                                            let drag_label_res = ui.selectable_label(false, "[Drag to Timeline]");
                                            
                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                if self.test_first_title_rect.is_none() {
                                                    self.test_first_title_rect = Some(drag_label_res.rect);
                                                }
                                            }

                                            if ui.interact(drag_label_res.rect, item_id, egui::Sense::drag()).dragged() {
                                                ctx.data_mut(|d| d.insert_temp(egui::Id::new("dragging_title_id"), title_id));
                                            }

                                            if ui.button("\u{2795}").on_hover_text("Add to Timeline").clicked() {
                                                add_to_timeline = true;
                                            }
                                        });

                                        if add_to_timeline {
                                            // Create a clip directly from title
                                            let clip = Clip {
                                                id: rand::random::<usize>(),
                                                asset_id: 0, // Titles use a special asset mapping or temp file
                                                track_id: 5,
                                                blend_mode: clipstrike_core::timeline::BlendMode::Normal,
                                                start: self.playhead_ms,
                                                end: self.playhead_ms + 5000,
                                                brightness: KeyframedProperty::new(1.0),
                                                contrast: KeyframedProperty::new(1.0),
                                                saturation: KeyframedProperty::new(1.0),
                                                grayscale: KeyframedProperty::new(0.0),
                                                blur_radius: KeyframedProperty::new(0.0),
                                                volume: KeyframedProperty::new(1.0),
                                                pos_x: KeyframedProperty::new(0.0),
                                                pos_y: KeyframedProperty::new(0.0),
                                                scale_x: KeyframedProperty::new(1.0),
                                                scale_y: KeyframedProperty::new(1.0),
                                                rotation: KeyframedProperty::new(0.0),
                                                opacity: KeyframedProperty::new(1.0),
                                                speed: KeyframedProperty::new(1.0),
                                                reverse: false,
                                                freeze_frame_ms: None,
                                                audio_enabled: false,
                                                video_enabled: true,
                                                stabilization_enabled: false,
                                                motion_tracking_enabled: false,
                                                object_detection_enabled: false,
                                                };                                            self.undo_stack.push(Box::new(AddClipCommand { clip }), &mut self.project, &mut self.timeline);
                                        }
                                    }
                                }
                            });
                        }
                        LeftPanelTab::Emojis => {
                            ui.label("Emoji / title placeholders");
                            ui.separator();
                            ui.label("Text overlays are not implemented yet.");
                        }
                    }
                });

            egui::SidePanel::right("properties_dock")
                .resizable(true)
                .min_width(280.0)
                .show(ctx, |ui| {
                    ui.heading("Properties");
                    ui.separator();

                    if let Some(selected_id) = self.selected_clip_id {
                        let playhead_ms = self.playhead_ms;
                        let mut selected_keyframe = self.selected_keyframe;
                        let mut should_clear_selection = false;
                        let mut selected_clip_range = None;
                        let mut selected_track_id = None;

                        if let Some(clip) =
                            self.timeline.clips.iter_mut().find(|c| c.id == selected_id)
                        {
                            selected_clip_range = Some((clip.start, clip.end));
                            selected_track_id = Some(clip.track_id);

                            ui.label(format!("Clip ID: {}", clip.id));
                            ui.add(
                                egui::Slider::new(&mut clip.start, 0..=100000).text("Start (ms)"),
                            );
                            ui.add(egui::Slider::new(&mut clip.end, 0..=100000).text("End (ms)"));

                            Self::draw_property_section(
                                ui,
                                clip,
                                selected_id,
                                playhead_ms,
                                &mut selected_keyframe,
                                "Visual Effects",
                                &AnimatedProperty::VISUALS,
                            );
                            Self::draw_property_section(
                                ui,
                                clip,
                                selected_id,
                                playhead_ms,
                                &mut selected_keyframe,
                                "Audio",
                                &AnimatedProperty::AUDIO,
                            );
                            Self::draw_property_section(
                                ui,
                                clip,
                                selected_id,
                                playhead_ms,
                                &mut selected_keyframe,
                                "Transform",
                                &AnimatedProperty::TRANSFORMS,
                            );

                            ui.horizontal(|ui| {
                                ui.label("Blend Mode");
                                egui::ComboBox::from_id_salt(("blend_mode", selected_id))
                                    .selected_text(format!("{:?}", clip.blend_mode))
                                    .show_ui(ui, |ui| {
                                        use clipstrike_core::timeline::BlendMode;
                                        for mode in [
                                            BlendMode::Normal,
                                            BlendMode::Multiply,
                                            BlendMode::Screen,
                                            BlendMode::Overlay,
                                            BlendMode::Darken,
                                            BlendMode::Lighten,
                                            BlendMode::Add,
                                            BlendMode::Subtract,
                                        ] {
                                            ui.selectable_value(&mut clip.blend_mode, mode, format!("{:?}", mode));
                                        }
                                    });
                            });

                            Self::draw_property_section(
                                ui,
                                clip,
                                selected_id,
                                playhead_ms,
                                &mut selected_keyframe,
                                "Time Remapping",
                                &AnimatedProperty::TIME_REMAP,
                            );
                            ui.checkbox(&mut clip.reverse, "Reverse Playback");
                            let current_source_ms = clip.source_time_at(playhead_ms);
                            ui.label(format!("Source Time at Playhead: {current_source_ms} ms"));
                            ui.horizontal(|ui| {
                                if ui.button("Freeze at Playhead").clicked() {
                                    clip.freeze_frame_ms = Some(current_source_ms);
                                }
                                if ui.button("Clear Freeze").clicked() {
                                    clip.freeze_frame_ms = None;
                                }
                            });
                            if let Some(freeze_frame_ms) = &mut clip.freeze_frame_ms {
                                ui.add(
                                    egui::DragValue::new(freeze_frame_ms)
                                        .speed(1)
                                        .prefix("Freeze Frame (ms): "),
                                );
                                if *freeze_frame_ms < 0 {
                                    *freeze_frame_ms = 0;
                                }
                            }

                            ui.separator();
                            ui.label("Transition Controls");
                            let fade_duration_ms = default_fade_duration_ms(clip.start, clip.end);
                            ui.horizontal(|ui| {
                                if ui
                                    .button(format!("Fade In ({fade_duration_ms} ms)"))
                                    .clicked()
                                {
                                    Self::apply_fade_in(clip);
                                }
                                if ui
                                    .button(format!("Fade Out ({fade_duration_ms} ms)"))
                                    .clicked()
                                {
                                    Self::apply_fade_out(clip);
                                }
                            });

                            if ui.button("Remove Clip").clicked() {
                                let clip = clip.clone();
                                self.undo_stack
                                    .push(Box::new(RemoveClipCommand { clip }), &mut self.project, &mut self.timeline);
                                should_clear_selection = true;
                            }
                        }

                        if !should_clear_selection {
                            if let (Some(track_id), Some((clip_start, clip_end))) =
                                (selected_track_id, selected_clip_range)
                            {
                                let overlapping_transitions: Vec<usize> = self
                                    .timeline
                                    .transitions
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, transition)| {
                                        transition.track_id == track_id
                                            && transition.start_ms < clip_end
                                            && transition.end_ms > clip_start
                                    })
                                    .map(|(index, _)| index)
                                    .collect();

                                if !overlapping_transitions.is_empty() {
                                    ui.separator();
                                    ui.label("Overlapping Transitions");
                                    for transition_index in overlapping_transitions {
                                        let mask_name = {
                                            let transition = &self.timeline.transitions[transition_index];
                                            transition
                                                .custom_mask_id
                                                .and_then(|mask_id| {
                                                    self.project
                                                        .transition_masks
                                                        .iter()
                                                        .find(|mask| mask.id == mask_id)
                                                })
                                                .map(|mask| mask.name.clone())
                                        };
                                        let range_label = {
                                            let transition = &self.timeline.transitions[transition_index];
                                            format!(
                                                "{} - {}",
                                                self.format_timecode(transition.start_ms),
                                                self.format_timecode(transition.end_ms)
                                            )
                                        };
                                        let transition = &mut self.timeline.transitions[transition_index];
                                        ui.group(|ui| {
                                            ui.label(range_label);
                                            egui::ComboBox::from_id_salt((
                                                "transition_type",
                                                selected_id,
                                                transition.id,
                                            ))
                                            .selected_text(match transition.transition_type {
                                                TransitionType::CustomMask => mask_name
                                                    .as_deref()
                                                    .map(|name| format!("Custom: {name}"))
                                                    .unwrap_or_else(|| "Custom Mask".to_string()),
                                                _ => transition.transition_type.label().to_string(),
                                            })
                                            .show_ui(ui, |ui| {
                                                for transition_type in [
                                                    TransitionType::Crossfade,
                                                    TransitionType::WipeLeft,
                                                    TransitionType::WipeRight,
                                                    TransitionType::WipeUp,
                                                    TransitionType::WipeDown,
                                                    TransitionType::WipeDiagonal,
                                                    TransitionType::WipeClock,
                                                ] {
                                                    if ui
                                                        .selectable_label(
                                                            transition.transition_type == transition_type,
                                                            transition_type.label(),
                                                        )
                                                        .clicked()
                                                    {
                                                        transition.transition_type = transition_type;
                                                        transition.custom_mask_id = None;
                                                    }
                                                }
                                                if !self.project.transition_masks.is_empty()
                                                    && ui
                                                        .selectable_label(
                                                            transition.transition_type == TransitionType::CustomMask,
                                                            "Custom Mask",
                                                        )
                                                        .clicked()
                                                {
                                                    transition.transition_type = TransitionType::CustomMask;
                                                    transition.custom_mask_id = transition
                                                        .custom_mask_id
                                                        .or_else(|| self.project.transition_masks.first().map(|mask| mask.id));
                                                }
                                            });

                                            if transition.transition_type == TransitionType::CustomMask {
                                                if let Some(name) = mask_name.as_deref() {
                                                    ui.label(format!("Mask: {name}"));
                                                } else {
                                                    ui.label("Mask: drop an imported luma mask onto the transition.");
                                                }
                                            }

                                            let snap_points = [transition.start_ms, transition.end_ms];
                                            Self::draw_transition_property_control(
                                                ui,
                                                transition.id,
                                                "Speed",
                                                &mut transition.speed,
                                                playhead_ms,
                                                0.25..=4.0,
                                                &snap_points,
                                            );
                                            Self::draw_transition_property_control(
                                                ui,
                                                transition.id,
                                                "Sharpness",
                                                &mut transition.softness,
                                                playhead_ms,
                                                0.0..=0.5,
                                                &snap_points,
                                            );
                                        });
                                    }
                                }
                            }
                        }

                        if should_clear_selection {
                            self.selected_clip_id = None;
                            self.selected_keyframe = None;
                        } else {
                            self.selected_keyframe = selected_keyframe;
                        }
                    } else {
                        ui.label("Select a clip to edit properties.");
                    }
                    ui.heading("Markers");
                    if self.project.markers.is_empty() {
                        ui.label("No markers.");
                    } else {
                        self.project.markers.sort_by_key(|marker| marker.time_ms);
                        let framerate = self.project.framerate;
                        let mut remove_marker_id = None;
                        let mut seek_to_marker: Option<(usize, i64)> = None;
                        for marker in &mut self.project.markers {
                            let selected = self.selected_marker_id == Some(marker.id);
                            let total_ms = marker.time_ms.max(0);
                            let hours = total_ms / 3_600_000;
                            let minutes = (total_ms / 60_000) % 60;
                            let seconds = (total_ms / 1_000) % 60;
                            let frames = (((total_ms % 1_000) as f64 / 1_000.0)
                                * framerate.max(1.0))
                            .floor() as i64;
                            let timecode =
                                format!("{:02}:{:02}:{:02}:{:02}", hours, minutes, seconds, frames);
                            ui.group(|ui| {
                                if ui
                                    .selectable_label(
                                        selected,
                                        format!("{}  {}", timecode, marker.label),
                                    )
                                    .clicked()
                                {
                                    seek_to_marker = Some((marker.id, marker.time_ms));
                                }
                                ui.text_edit_singleline(&mut marker.label);
                                let mut color = Self::marker_color(marker);
                                if ui.color_edit_button_srgba(&mut color).changed() {
                                    Self::set_marker_color(marker, color);
                                }
                                if ui.button("Remove Marker").clicked() {
                                    remove_marker_id = Some(marker.id);
                                }
                            });
                        }
                        if let Some((marker_id, marker_time_ms)) = seek_to_marker {
                            self.selected_marker_id = Some(marker_id);
                            self.playhead_ms = marker_time_ms;
                            self.seek_playhead();
                        }
                        if let Some(marker_id) = remove_marker_id {
                            self.project.markers.retain(|marker| marker.id != marker_id);
                            if self.selected_marker_id == Some(marker_id) {
                                self.selected_marker_id = None;
                            }
                        }
                    }
                });
        }
        // CENTRAL PANEL: Video Preview
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                // Header / Title for Preview
                ui.label("Video Preview");
                ui.separator();

                // Video Display Area
                let available_size = ui.available_size() - egui::vec2(0.0, 40.0); // Leave room for transport controls
                let (rect, _) = ui.allocate_exact_size(available_size, egui::Sense::hover());

                ui.painter().rect_filled(rect, 4.0, egui::Color32::BLACK);

                // Draw active clips using the CompositorPipeline for professional track blending.
                let mut frame_rendered = false;
                if let (
                    Some(device),
                    Some(queue),
                    Some(fx_pipeline),
                    Some(compositor_pipeline),
                    Some(fx_output_view),
                    Some(accumulator_view),
                    Some(scratch_view),
                ) = (
                    &self.device,
                    &self.queue,
                    &self.fx_pipeline,
                    &self.compositor_pipeline,
                    &self.fx_output_view,
                    &self.accumulator_view,
                    &self.scratch_view,
                ) {
                    // Clear accumulator to black at the start of the frame
                    let mut encoder =
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Clear Accumulator"),
                        });
                    {
                        let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("Clear Pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: accumulator_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                        });
                    }
                    queue.submit(Some(encoder.finish()));

                    if let Some(texture_pool) = &self.texture_pool {
                        if let Ok(pool) = texture_pool.lock() {
                            let mut sorted_tracks = self.project.tracks.clone();
                            sorted_tracks.sort_by_key(|t| t.id);

                            for track in sorted_tracks {
                                if !track.visible {
                                    continue;
                                }

                                let track_clips: Vec<Clip> = self
                                    .timeline
                                    .get_clips_at_time(self.playhead_ms)
                                    .into_iter()
                                    .filter(|c| c.track_id == track.id)
                                    .collect();

                                if track_clips.is_empty() {
                                    continue;
                                }

                                // Use the blend_mode from the top-most clip on that track
                                let blend_mode = track_clips
                                    .last()
                                    .map(|c| c.blend_mode)
                                    .unwrap_or(clipstrike_core::timeline::BlendMode::Normal);

                                for clip in track_clips {
                                    if let Some(frame) = pool.frame_for_clip(clip.id) {
                                        let fx_params = FxUniforms {
                                            brightness: clip.brightness.get_value_at(self.playhead_ms),
                                            contrast: clip.contrast.get_value_at(self.playhead_ms),
                                            saturation: clip.saturation.get_value_at(self.playhead_ms),
                                            grayscale: clip.grayscale.get_value_at(self.playhead_ms),
                                            blur_radius: clip.blur_radius.get_value_at(self.playhead_ms),
                                            alpha: clip.opacity.get_value_at(self.playhead_ms),
                                            ..Default::default()
                                        };
                                        fx_pipeline.run(
                                            device,
                                            queue,
                                            &frame.view,
                                            fx_output_view,
                                            fx_params,
                                        );

                                        let compose_params = ComposeUniforms {
                                            blend_mode: Self::blend_mode_to_u32(blend_mode),
                                            opacity: 1.0,
                                        };
                                        compositor_pipeline.run(
                                            device,
                                            queue,
                                            fx_output_view,
                                            accumulator_view,
                                            scratch_view,
                                            compose_params,
                                        );

                                        // Copy scratch back to accumulator to avoid resource aliasing in next track
                                        let mut copy_encoder = device.create_command_encoder(
                                            &wgpu::CommandEncoderDescriptor {
                                                label: Some("Copy Scratch to Accum"),
                                            },
                                        );
                                        copy_encoder.copy_texture_to_texture(
                                            wgpu::ImageCopyTexture {
                                                texture: self._scratch_texture.as_ref().unwrap(),
                                                mip_level: 0,
                                                origin: wgpu::Origin3d::ZERO,
                                                aspect: wgpu::TextureAspect::All,
                                            },
                                            wgpu::ImageCopyTexture {
                                                texture: self._accumulator_texture.as_ref().unwrap(),
                                                mip_level: 0,
                                                origin: wgpu::Origin3d::ZERO,
                                                aspect: wgpu::TextureAspect::All,
                                            },
                                            wgpu::Extent3d {
                                                width: 1920,
                                                height: 1080,
                                                depth_or_array_layers: 1,
                                            },
                                        );
                                        queue.submit(Some(copy_encoder.finish()));
                                        frame_rendered = true;
                                    }
                                }
                            }
                        }
                    }
                }

                if frame_rendered {
                    ui.painter().image(
                        self.accumulator_id.unwrap(),
                        rect,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        PREVIEW_IDLE_MESSAGE,
                        egui::FontId::proportional(20.0),
                        egui::Color32::DARK_GRAY,
                    );
                }

                // Transport controls below video
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let frame_duration = self.frame_duration_ms();
                        let timeline_end = self.timeline_end_ms();

                        if ui.button("|<").on_hover_text("Jump to Start").clicked() {
                            self.playhead_ms = 0;
                            self.seek_playhead();
                            if let Some(audio) = &self.audio_engine {
                                audio.set_playhead(self.playhead_ms);
                            }
                        }
                        if ui.button("<<").on_hover_text("Previous Frame").clicked() {
                            self.playhead_ms = (self.playhead_ms - frame_duration).max(0);
                            self.seek_playhead();
                            if let Some(audio) = &self.audio_engine {
                                audio.set_playhead(self.playhead_ms);
                            }
                        }
                        let play_icon = if self.is_playing { "Pause" } else { "Play" };
                        if ui.button(play_icon).clicked() {
                            self.is_playing = !self.is_playing;
                            self.seek_playhead();
                            if let Some(audio) = &self.audio_engine {
                                audio.set_playing(self.is_playing);
                            }
                        }
                        if ui.button(">>").on_hover_text("Next Frame").clicked() {
                            self.playhead_ms =
                                (self.playhead_ms + frame_duration).min(timeline_end);
                            self.seek_playhead();
                            if let Some(audio) = &self.audio_engine {
                                audio.set_playhead(self.playhead_ms);
                            }
                        }
                        ui.toggle_value(&mut self.loop_enabled, "Loop")
                            .on_hover_text("Toggle Loop");
                        if ui.button(">|").on_hover_text("Jump to End").clicked() {
                            self.playhead_ms = timeline_end;
                            self.seek_playhead();
                            if let Some(audio) = &self.audio_engine {
                                audio.set_playhead(self.playhead_ms);
                            }
                        }

                        ui.label(format!(
                            "{} / {}",
                            self.format_timecode(self.playhead_ms),
                            self.format_timecode(timeline_end),
                        ));

                        ui.separator();
                        ui.label("\u{1F50A}");
                        if ui.add(egui::Slider::new(&mut self.master_volume, 0.0..=2.0).show_value(false)).changed() {
                            self.project.master_volume = self.master_volume;
                            if let Some(audio) = &self.audio_engine {
                                audio.set_volume(self.master_volume);
                            }
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("Save Frame")
                                .on_hover_text("Save Current Frame")
                                .clicked()
                            {
                                if let Some(path) = self.pick_frame_save_path() {
                                    if let Err(err) = self.save_current_frame_to_path(&path) {
                                        eprintln!("{err}");
                                    }
                                }
                            }
                        });
                    });
                });
            });
        });

        self.draw_title_editor(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipstrike_core::clip_defaults::timeline_clip_end_ms;
    use clipstrike_core::keyframes::KeyframedProperty;

    #[test]
    fn animated_property_labels_are_correct() {
        assert_eq!(AnimatedProperty::Brightness.label(), "Brightness");
        assert_eq!(AnimatedProperty::PosX.label(), "Position X");
        assert_eq!(AnimatedProperty::Volume.label(), "Volume");
    }

    #[test]
    fn animated_property_ranges_are_correct() {
        assert_eq!(AnimatedProperty::Opacity.range(), (0.0, 1.0));
        assert_eq!(AnimatedProperty::Rotation.range(), (-180.0, 180.0));
        assert_eq!(AnimatedProperty::ScaleX.range(), (0.0, 5.0));
    }

    fn make_test_clip() -> Clip {
        Clip {
            id: 1,
            asset_id: 1,
            track_id: 1,
            blend_mode: clipstrike_core::timeline::BlendMode::Normal,
            start: 0,
            end: 1000,
            brightness: KeyframedProperty::new(1.0),
            contrast: KeyframedProperty::new(1.0),
            saturation: KeyframedProperty::new(1.0),
            grayscale: KeyframedProperty::new(0.0),
            blur_radius: KeyframedProperty::new(0.0),
            volume: KeyframedProperty::new(1.0),
            pos_x: KeyframedProperty::new(0.0),
            pos_y: KeyframedProperty::new(0.0),
            scale_x: KeyframedProperty::new(1.0),
            scale_y: KeyframedProperty::new(1.0),
            rotation: KeyframedProperty::new(0.0),
            opacity: KeyframedProperty::new(1.0),
            speed: KeyframedProperty::new(1.0),
            reverse: false,
            freeze_frame_ms: None,
            audio_enabled: false,
            video_enabled: true,
            stabilization_enabled: false,
            motion_tracking_enabled: false,
            object_detection_enabled: false,
            };
    }

    #[test]
    fn property_accessors_map_correctly() {
        let mut clip = make_test_clip();

        // Test immutable ref
        assert_eq!(property_ref(&clip, AnimatedProperty::Brightness).get_value_at(0), 1.0);
        
        // Test mutable ref
        property_mut(&mut clip, AnimatedProperty::Brightness)
            .insert_keyframe(0, 2.5, InterpolationMode::Linear);
        
        assert_eq!(property_ref(&clip, AnimatedProperty::Brightness).get_value_at(0), 2.5);
    }
}
