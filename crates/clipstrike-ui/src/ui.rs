use crate::app_config::{AppConfig, ProjectProfile, ThemePreference};
use crate::export::{ExportProgress, export_frame_count_for_duration_ms};
use clipstrike_audio::waveform::extract_waveform_sync;
use clipstrike_core::bridges::inkscape::InkscapeBridge;
use clipstrike_core::clip_defaults::{clamp_trim_to_asset_bounds, timeline_clip_end_ms};
use clipstrike_core::commands::{
    AddClipCommand, CompoundCommand, MoveClipCommand, RemoveClipCommand, SeparateAudioCommand, TrimClipCommand,
    UndoStack,
};
use clipstrike_media::engine_manager::EngineManager;
use clipstrike_media::export::{ExportFormat, ExportSettings, Exporter};
use clipstrike_media::probe::probe_metadata_sync;
use clipstrike_render::fx::{ComposeUniforms, CompositorPipeline, FxPipeline, FxUniforms};
use clipstrike_render::gpu::{
    FrameSourceKind, FrameTexture, FrameUploadRequest, TexturePool, read_texture_rgba_into,
};

use clipstrike_core::effects::{DetectedObject, MotionTrack, TrackPoint};
use clipstrike_core::keyframes::{DEFAULT_KEYFRAME_SNAP_MS, InterpolationMode, KeyframedProperty};
use clipstrike_core::labels::{timeline_clip_label, timeline_clip_tooltip};
use clipstrike_core::playback::{advance_playhead, clip_eos_step};
use clipstrike_core::project::{
    Marker, MarkerToggleResult, MediaAsset, Project, Track, TransitionMaskAsset,
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
use glam::Vec3;
#[cfg(not(target_arch = "wasm32"))]
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::cell::OnceCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

const TRACK_HEADER_WIDTH: f32 = 132.0;
const TRACK_HEIGHT: f32 = 80.0;
const TIMELINE_CONTENT_OFFSET_X: f32 = TRACK_HEADER_WIDTH + 16.0;

#[derive(PartialEq, Debug)]
pub enum LeftPanelTab {
    ProjectFiles,
    Transitions,
    Effects,
    Titles,
    Emojis,
}

#[allow(dead_code)]
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
    wgpu_render_state: Option<eframe::egui_wgpu::RenderState>,
    engine_manager: Option<EngineManager>,
    audio_engine: Option<clipstrike_audio::PlaybackEngine>,
    fx_pipeline: Option<FxPipeline>,
    compositor_pipeline: Option<CompositorPipeline>,
    video_cache: clipstrike_render::cache::VideoCache,
    texture_pool: Option<TexturePool>,
    frame_upload_rx: Option<tokio::sync::mpsc::Receiver<FrameUploadRequest>>,
    _fx_output_texture: Option<wgpu::Texture>,
    fx_output_view: Option<wgpu::TextureView>,
    fx_output_id: Option<egui::TextureId>,
    fx_sampler: OnceCell<wgpu::Sampler>,

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
    drag_pointer_start_y: Option<f32>,
    drag_clip_id: Option<usize>,
    dragged_group_clip_ids: Vec<usize>,
    drag_start_track_index: Option<usize>,
    drag_preview_times: Option<(i64, i64)>,
    drag_preview_track_index: Option<usize>,
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
    show_export_dialog: bool,
    track_name_edit_id: Option<usize>,
    track_name_buffer: String,
    save_frame_buffer: Vec<u8>,
    export_preset_index: usize,
    export_output_path: Option<PathBuf>,
    export_notification: Option<String>,
    active_export: Option<ActiveExportJob>,

    // Test Hooks Bounding Boxes
    test_first_track_rect: Option<egui::Rect>,
    test_first_asset_rect: Option<egui::Rect>,
    test_first_clip_rect: Option<egui::Rect>,
    test_first_title_rect: Option<egui::Rect>,
    test_project_json_dump: Option<String>,
    test_media_picker_paths: Option<Vec<PathBuf>>,
    #[cfg(not(target_arch = "wasm32"))]
    title_svg_edit_sessions: HashMap<usize, TitleEditSession>,
    was_playing_before_scrub: bool,
    last_track_content_origin_x: Option<f32>,
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

struct ActiveExportJob {
    output_path: PathBuf,
    exporter: Exporter,
    progress: ExportProgress,
    settings: ExportSettings,
    next_frame_ms: i64,
    frame_duration_ms: i64,
    pending_capture: bool,
    capture_ready: bool,
    started_at: Instant,
    restore_playhead_ms: i64,
    restore_is_playing: bool,
}

#[cfg(not(target_arch = "wasm32"))]
struct TitleEditSession {
    svg_path: PathBuf,
    watcher: RecommendedWatcher,
    events: std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
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
    PixelateSize,
    ColorTintR,
    ColorTintG,
    ColorTintB,
}

impl AnimatedProperty {
    pub const VISUALS: [Self; 9] = [
        Self::Brightness,
        Self::Contrast,
        Self::Saturation,
        Self::Grayscale,
        Self::BlurRadius,
        Self::PixelateSize,
        Self::ColorTintR,
        Self::ColorTintG,
        Self::ColorTintB,
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
            Self::PixelateSize => "Pixelate Size",
            Self::ColorTintR => "Tint Red",
            Self::ColorTintG => "Tint Green",
            Self::ColorTintB => "Tint Blue",
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
            Self::PixelateSize => (1.0, 100.0),
            Self::ColorTintR | Self::ColorTintG | Self::ColorTintB => (0.0, 2.0),
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
        AnimatedProperty::PixelateSize => &clip.pixelate_size,
        AnimatedProperty::ColorTintR => &clip.color_tint_r,
        AnimatedProperty::ColorTintG => &clip.color_tint_g,
        AnimatedProperty::ColorTintB => &clip.color_tint_b,
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
        AnimatedProperty::PixelateSize => &mut clip.pixelate_size,
        AnimatedProperty::ColorTintR => &mut clip.color_tint_r,
        AnimatedProperty::ColorTintG => &mut clip.color_tint_g,
        AnimatedProperty::ColorTintB => &mut clip.color_tint_b,
    }
}

fn parse_shortcut_spec(spec: &str) -> Option<egui::KeyboardShortcut> {
    let mut modifiers = egui::Modifiers::NONE;
    let mut key = None;

    for token in spec
        .split('+')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "command" => {
                modifiers = modifiers | egui::Modifiers::COMMAND
            }
            "shift" => modifiers = modifiers | egui::Modifiers::SHIFT,
            "alt" | "option" => modifiers = modifiers | egui::Modifiers::ALT,
            "space" => key = Some(egui::Key::Space),
            other => {
                key = egui::Key::from_name(token).or_else(|| {
                    if token.len() == 1 {
                        egui::Key::from_name(&token.to_uppercase())
                    } else {
                        egui::Key::from_name(other)
                    }
                });
            }
        }
    }

    key.map(|logical_key| egui::KeyboardShortcut::new(modifiers, logical_key))
}

fn shortcut_consumed(ctx: &egui::Context, spec: &str) -> bool {
    if ctx.wants_keyboard_input() {
        return false;
    }
    parse_shortcut_spec(spec)
        .map(|shortcut| ctx.input_mut(|i| i.consume_shortcut(&shortcut)))
        .unwrap_or(false)
}

#[allow(dead_code)]
impl ClipStrikeApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let app_config = AppConfig::load_or_default();
        let wgpu_render_state = cc.wgpu_render_state.clone();

        if let Some(state) = &wgpu_render_state {
            let info = state.adapter.get_info();
            log::info!("WGPU adapter: {} ({:?})", info.name, info.backend);
        }

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
            frame_upload_rx,
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
                        frame.preview_view(),
                        wgpu::FilterMode::Linear,
                    ));
                }
            }

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
            let fx_output_view =
                fx_output_texture.create_view(&wgpu::TextureViewDescriptor::default());
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

            let fx_pipeline = FxPipeline::new(&device, &queue);
            let compositor_pipeline = CompositorPipeline::new(&device);
            let audio_engine = clipstrike_audio::PlaybackEngine::new().ok();
            let (frame_upload_tx, frame_upload_rx) = tokio::sync::mpsc::channel(32);
            #[cfg(not(target_arch = "wasm32"))]
            let engine_manager = EngineManager::new(
                app_config.default_project_profile.width as i32,
                app_config.default_project_profile.height as i32,
                Some(frame_upload_tx),
                audio_engine.as_ref().map(|engine| engine.handle()),
            );
            #[cfg(target_arch = "wasm32")]
            let engine_manager = EngineManager::new(
                app_config.default_project_profile.width as i32,
                app_config.default_project_profile.height as i32,
                audio_engine.as_ref().map(|engine| engine.handle()),
            );

            (
                Some(device),
                Some(queue),
                Some(pool),
                Some(frame_upload_rx),
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
            (
                None, None, None, None, None, None, None, None, None, None, None, None, None, None,
                None, None,
            )
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
            wgpu_render_state,
            engine_manager,
            audio_engine,
            fx_pipeline,
            compositor_pipeline,
            video_cache: clipstrike_render::cache::VideoCache::new(60),
            texture_pool,
            frame_upload_rx,
            _fx_output_texture,
            fx_output_view,
            fx_output_id,
            fx_sampler: OnceCell::new(),
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
            drag_pointer_start_y: None,
            drag_clip_id: None,
            dragged_group_clip_ids: Vec::new(),
            drag_start_track_index: None,
            drag_preview_times: None,
            drag_preview_track_index: None,
            trim_side: None,
            test_first_track_rect: None,
            test_first_asset_rect: None,
            test_first_clip_rect: None,
            test_first_title_rect: None,
            test_project_json_dump: None,
            test_media_picker_paths: None,
            #[cfg(not(target_arch = "wasm32"))]
            title_svg_edit_sessions: HashMap::new(),
            loop_enabled: false,
            loop_start_ms: 0,
            loop_end_ms: 10000,
            asset_filter_text: String::new(),
            asset_filter_index: 0,
            master_volume,
            show_preferences_dialog: false,
            show_profile_dialog: false,
            show_export_dialog: false,
            track_name_edit_id: None,
            track_name_buffer: String::new(),
            save_frame_buffer: Vec::new(),
            export_preset_index: 0,
            export_output_path: None,
            export_notification: None,
            active_export: None,
            show_title_editor: false,
            editing_title: None,
            was_playing_before_scrub: false,
            last_track_content_origin_x: None,
        }
    }

    pub fn test_harness(mut project: Project) -> Self {
        project.tracks = Project::default_tracks(5);
        let master_volume = project.master_volume;
        let timeline = project.to_timeline();

        Self {
            left_panel_tab: LeftPanelTab::ProjectFiles,
            zoom_level: 0.5,
            timeline,
            app_config: AppConfig::default(),
            project,
            current_project_path: None,
            undo_stack: UndoStack::new(),
            device: None,
            queue: None,
            wgpu_render_state: None,
            engine_manager: None,
            audio_engine: None,
            fx_pipeline: None,
            compositor_pipeline: None,
            video_cache: clipstrike_render::cache::VideoCache::new(60),
            texture_pool: None,
            frame_upload_rx: None,
            _fx_output_texture: None,
            fx_output_view: None,
            fx_output_id: None,
            fx_sampler: OnceCell::new(),
            _accumulator_texture: None,
            accumulator_view: None,
            accumulator_id: None,
            _scratch_texture: None,
            scratch_view: None,
            is_playing: false,
            current_time_ms: 0,
            playhead_ms: 0,
            selected_clip_id: None,
            selected_marker_id: None,
            selected_keyframe: None,
            razor_active: false,
            drag_start_times: None,
            drag_pointer_start_x: None,
            drag_pointer_start_y: None,
            drag_clip_id: None,
            dragged_group_clip_ids: Vec::new(),
            drag_start_track_index: None,
            drag_preview_times: None,
            drag_preview_track_index: None,
            master_volume,
            show_title_editor: false,
            editing_title: None,
            trim_side: None,
            loop_enabled: false,
            loop_start_ms: 0,
            loop_end_ms: 10000,
            asset_filter_text: String::new(),
            asset_filter_index: 0,
            show_preferences_dialog: false,
            show_profile_dialog: false,
            show_export_dialog: false,
            track_name_edit_id: None,
            track_name_buffer: String::new(),
            save_frame_buffer: Vec::new(),
            export_preset_index: 0,
            export_output_path: None,
            export_notification: None,
            active_export: None,
            test_first_track_rect: None,
            test_first_asset_rect: None,
            test_first_clip_rect: None,
            test_first_title_rect: None,
            test_project_json_dump: None,
            test_media_picker_paths: None,
            #[cfg(not(target_arch = "wasm32"))]
            title_svg_edit_sessions: HashMap::new(),
            was_playing_before_scrub: false,
            last_track_content_origin_x: None,
        }
    }

    pub fn test_harness_with_frame_upload_rx(
        mut project: Project,
        frame_upload_rx: tokio::sync::mpsc::Receiver<FrameUploadRequest>,
    ) -> Self {
        project.tracks = Project::default_tracks(5);
        let mut app = Self::test_harness(project);
        app.frame_upload_rx = Some(frame_upload_rx);
        app
    }

    pub fn set_test_media_picker_paths(&mut self, paths: Vec<PathBuf>) {
        self.test_media_picker_paths = Some(paths);
    }

    pub fn clear_test_media_picker_paths(&mut self) {
        self.test_media_picker_paths = None;
    }

    fn collect_pending_frame_upload_requests(&mut self) -> Vec<FrameUploadRequest> {
        let mut requests = Vec::new();
        if let Some(receiver) = &mut self.frame_upload_rx {
            while let Ok(request) = receiver.try_recv() {
                requests.push(request);
            }
        }
        requests
    }

    fn add_test_clip_to_first_track(&mut self) {
        let track_id = self
            .project
            .tracks
            .first()
            .map(|track| track.id)
            .unwrap_or(0);
        let clip = Clip::new(rand::random::<usize>(), 1, track_id, 0, 1000, 0, 1000);
        self.undo_stack.push(
            Box::new(AddClipCommand { clip }),
            &mut self.project,
            &mut self.timeline,
        );
    }

    fn default_drop_track_id(&self) -> Option<usize> {
        self.visible_tracks_in_render_order()
            .next()
            .map(|track| track.id)
            .or_else(|| self.project.tracks.first().map(|track| track.id))
    }

    fn add_asset_to_timeline(
        &mut self,
        asset_id: usize,
        asset_duration_ms: u64,
    ) -> Result<(), String> {
        let Some(track_id) = self.default_drop_track_id() else {
            log::error!(
                "No visible tracks available for adding asset {} duration_ms={}",
                asset_id,
                asset_duration_ms
            );
            return Err("No visible tracks available for adding media".to_string());
        };
        log::info!(
            "Adding asset {} to timeline on track {} at playhead {} for {} ms",
            asset_id,
            track_id,
            self.playhead_ms,
            asset_duration_ms
        );

        let end_ms = timeline_clip_end_ms(
            self.playhead_ms,
            asset_duration_ms,
            self.frame_duration_ms(),
        );
        let mut clip = Clip::new(
            rand::random::<usize>(),
            asset_id,
            track_id,
            self.playhead_ms,
            end_ms,
            0,
            asset_duration_ms as i64,
        );
        clip.video_enabled = true;
        clip.audio_enabled = self
            .project
            .assets
            .iter()
            .find(|asset| asset.id == asset_id)
            .map(Self::media_asset_has_audio)
            .unwrap_or(false);
        self.undo_stack.push(
            Box::new(AddClipCommand { clip }),
            &mut self.project,
            &mut self.timeline,
        );
        Ok(())
    }

    fn materialize_title_asset(&mut self, title_id: usize) -> Result<usize, String> {
        let title = self
            .project
            .titles
            .iter()
            .find(|candidate| candidate.id == title_id)
            .cloned()
            .ok_or_else(|| format!("Title {title_id} not found"))?;

        #[cfg(not(target_arch = "wasm32"))]
        let uri = {
            let svg_path = Self::title_svg_temp_path(title.id);
            if let Some(parent) = svg_path.parent() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    format!(
                        "Failed to create title asset directory {}: {err}",
                        parent.display()
                    )
                })?;
            }
            std::fs::write(&svg_path, &title.svg_content).map_err(|err| {
                format!(
                    "Failed to write title asset SVG {}: {err}",
                    svg_path.display()
                )
            })?;
            Self::file_uri_from_path(&svg_path)?
        };

        #[cfg(target_arch = "wasm32")]
        let uri = {
            use base64::{Engine as _, engine::general_purpose};
            let b64 = general_purpose::STANDARD.encode(&title.svg_content);
            format!("data:image/svg+xml;base64,{}", b64)
        };

        if let Some(existing_asset) = self.project.assets.iter_mut().find(|asset| asset.uri == uri) {
            existing_asset.name = title.name.clone();
            existing_asset.duration_ms = existing_asset.duration_ms.max(10_000);
            existing_asset.audio_levels.clear();
            return Ok(existing_asset.id);
        }

        let asset_id = rand::random::<usize>();
        self.project.assets.push(MediaAsset {
            id: asset_id,
            name: title.name,
            uri,
            duration_ms: 10_000,
            audio_levels: Vec::new(),
        });
        Ok(asset_id)
    }

    fn create_title_clip(
        &mut self,
        title_id: usize,
        track_id: usize,
        start_ms: i64,
        clip_duration_ms: u64,
    ) -> Result<Clip, String> {
        let asset_id = self.materialize_title_asset(title_id)?;
        let end_ms = timeline_clip_end_ms(start_ms, clip_duration_ms, self.frame_duration_ms());
        let mut clip = Clip::new(
            rand::random::<usize>(),
            asset_id,
            track_id,
            start_ms,
            end_ms,
            0,
            clip_duration_ms as i64,
        );
        clip.video_enabled = true;
        clip.audio_enabled = false;
        Ok(clip)
    }

    fn add_title_to_timeline(
        &mut self,
        title_id: usize,
        clip_duration_ms: u64,
    ) -> Result<(), String> {
        let Some(track_id) = self.default_drop_track_id() else {
            return Err("No visible tracks available for adding titles".to_string());
        };
        let clip = self.create_title_clip(title_id, track_id, self.playhead_ms, clip_duration_ms)?;
        self.undo_stack.push(
            Box::new(AddClipCommand { clip }),
            &mut self.project,
            &mut self.timeline,
        );
        Ok(())
    }

    fn asset_library_kind(uri: &str) -> &'static str {
        let ext = uri.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "mp4" | "mov" | "mkv" | "webm" | "avi" => "Video",
            "wav" | "mp3" | "aac" | "flac" | "ogg" => "Audio",
            "png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" => "Image",
            _ => "Media",
        }
    }

    fn asset_drag_preview_accent(kind: &str) -> egui::Color32 {
        match kind {
            "Video" => egui::Color32::from_rgb(34, 139, 230),
            "Audio" => egui::Color32::from_rgb(242, 153, 74),
            "Image" => egui::Color32::from_rgb(39, 174, 96),
            "Title" => egui::Color32::from_rgb(219, 68, 55),
            _ => egui::Color32::from_rgb(155, 166, 178),
        }
    }

    fn draw_library_drag_preview(&self, ctx: &egui::Context) {
        let Some(pointer_pos) = ctx.input(|input| input.pointer.interact_pos()) else {
            return;
        };

        let preview = if let Some(asset_id) = ctx.data(|data| data.get_temp::<usize>(egui::Id::new("dragging_asset_id"))) {
            self.project.assets.iter().find(|asset| asset.id == asset_id).map(|asset| {
                let kind = Self::asset_library_kind(&asset.uri);
                (kind, asset.name.clone(), Self::asset_drag_preview_accent(kind))
            })
        } else if let Some(title_id) = ctx.data(|data| data.get_temp::<usize>(egui::Id::new("dragging_title_id"))) {
            self.project.titles.iter().find(|title| title.id == title_id).map(|title| {
                ("Title", title.name.clone(), Self::asset_drag_preview_accent("Title"))
            })
        } else {
            None
        };

        let Some((kind, name, accent)) = preview else {
            return;
        };

        let card_rect = egui::Rect::from_min_size(
            pointer_pos + egui::vec2(18.0, 18.0),
            egui::vec2(240.0, 58.0),
        );
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("library_drag_preview"),
        ));
        let fill = egui::Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 110);
        let stroke = egui::Stroke::new(
            1.0,
            egui::Color32::from_rgba_premultiplied(accent.r(), accent.g(), accent.b(), 210),
        );
        let text_color = ctx.style().visuals.text_color();

        painter.rect_filled(card_rect, 12.0, fill);
        painter.rect_stroke(card_rect, 12.0, stroke);
        painter.rect_filled(
            egui::Rect::from_min_max(
                card_rect.left_top(),
                egui::pos2(card_rect.left() + 8.0, card_rect.bottom()),
            ),
            12.0,
            accent,
        );
        painter.text(
            card_rect.left_top() + egui::vec2(18.0, 10.0),
            egui::Align2::LEFT_TOP,
            kind,
            egui::FontId::proportional(12.0),
            accent,
        );
        painter.text(
            card_rect.left_top() + egui::vec2(18.0, 28.0),
            egui::Align2::LEFT_TOP,
            name,
            egui::FontId::proportional(16.0),
            text_color,
        );
    }

    fn split_audio_target_track_id(track_ids: &[usize], clip_track_id: usize) -> Option<usize> {
        track_ids
            .iter()
            .copied()
            .filter(|track_id| *track_id > clip_track_id)
            .min()
            .or_else(|| {
                track_ids
                    .iter()
                    .copied()
                    .filter(|track_id| *track_id < clip_track_id)
                    .max()
            })
    }

    fn apply_motion_track_test(&mut self) {
        if let Some(clip) = self.timeline.clips.first_mut() {
            clip.motion_tracking_enabled = true;
            clip.tracking_data = vec![MotionTrack {
                id: rand::random::<usize>(),
                name: "Test Motion Track".to_string(),
                points: vec![
                    TrackPoint {
                        time_ms: clip.start,
                        x: 12.0,
                        y: 18.0,
                        width: 96.0,
                        height: 72.0,
                    },
                    TrackPoint {
                        time_ms: clip.start + 500,
                        x: 24.0,
                        y: 30.0,
                        width: 96.0,
                        height: 72.0,
                    },
                    TrackPoint {
                        time_ms: clip.start + 1000,
                        x: 36.0,
                        y: 42.0,
                        width: 96.0,
                        height: 72.0,
                    },
                ],
            }];
            clip.pos_x = KeyframedProperty::new(0.0);
            clip.pos_y = KeyframedProperty::new(0.0);
            for point in &clip.tracking_data[0].points {
                clip.pos_x
                    .insert_keyframe(point.time_ms, point.x, InterpolationMode::Linear);
                clip.pos_y
                    .insert_keyframe(point.time_ms, point.y, InterpolationMode::Linear);
            }
        }
    }

    fn apply_detection_labels_test(&mut self) {
        if let Some(clip) = self.timeline.clips.first_mut() {
            clip.object_detection_enabled = true;
            clip.detections = vec![DetectedObject {
                label: "Test Object".to_string(),
                confidence: 0.99,
                x: 24.0,
                y: 16.0,
                width: 48.0,
                height: 40.0,
            }];
        }
    }

    fn render_test_imported_assets_list(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.project.assets.is_empty() {
                ui.label("Drop Files Here...");
            } else {
                let asset_ids: Vec<usize> =
                    self.project.assets.iter().map(|asset| asset.id).collect();

                for id in asset_ids {
                    let asset_data = {
                        let asset = self.project.assets.iter().find(|a| a.id == id).unwrap();
                        (
                            asset.id,
                            asset.name.clone(),
                            asset.uri.clone(),
                            asset.duration_ms,
                        )
                    };
                    let (asset_id, asset_name, asset_uri, asset_duration_ms) = asset_data;
                    let item_id = egui::Id::new(("asset", asset_id));

                    let mut add_to_timeline = false;
                    ui.horizontal(|ui| {
                        ui.label(Self::asset_library_kind(&asset_uri));
                        let label_res = ui.selectable_label(false, &asset_name);
                        if ui
                            .interact(label_res.rect, item_id, egui::Sense::drag())
                            .dragged()
                        {
                            ctx.data_mut(|d| {
                                d.insert_temp(egui::Id::new("dragging_asset_id"), asset_id)
                            });
                        }

                        if ui.button("+").on_hover_text("Add to Timeline").clicked() {
                            add_to_timeline = true;
                        }
                    });

                    if add_to_timeline {
                        if let Err(err) = self.add_asset_to_timeline(asset_id, asset_duration_ms) {
                            log::error!("Failed to add asset {} to timeline: {}", asset_id, err);
                        }
                    }
                }
            }
        });
    }

    pub fn render_project_files_test_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Import").clicked() {
                if let Some(paths) = self.pick_media_files() {
                    self.import_media_paths(paths);
                }
            }
            if ui.button("Add Test Clip").clicked() {
                self.add_test_clip_to_first_track();
            }
            if ui.button("Apply Motion Track (Test)").clicked() {
                self.apply_motion_track_test();
            }
            if ui.button("Apply Detection Labels (Test)").clicked() {
                self.apply_detection_labels_test();
            }
        });
        let ctx = ui.ctx().clone();
        self.render_test_imported_assets_list(ui, &ctx);
        if let Some(message) = self.recovery_banner_message() {
            ui.horizontal(|ui| {
                ui.label(message);
                if ui.button("Restore Recovery Checkpoint").clicked() {
                    if let Err(err) = self.restore_recovery_checkpoint() {
                        log::error!("Failed to restore recovery checkpoint: {}", err);
                    }
                }
            });
        }

        let snapshot = self.test_snapshot();
        ui.label(format!(
            "__clipstrike_test_clip_count__: {}",
            snapshot["clips_count"].as_u64().unwrap_or(0)
        ));
        ui.label(format!(
            "__clipstrike_test_motion_track_count__: {}",
            snapshot["tracking_data_count"].as_u64().unwrap_or(0)
        ));
        ui.label(format!(
            "__clipstrike_test_detection_label_count__: {}",
            snapshot["detection_count"].as_u64().unwrap_or(0)
        ));
        ui.label(format!(
            "__clipstrike_test_playback_complete__: {}",
            snapshot["playback_complete"].as_bool().unwrap_or(false)
        ));
    }

    pub fn render_transport_test_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let play_icon = if self.is_playing { "Pause" } else { "Play" };
            if ui.button(play_icon).clicked() {
                self.toggle_playback();
            }
        });
    }
    pub fn test_snapshot(&self) -> serde_json::Value {
        let timeline_end_ms = self.timeline_end_ms();
        let playback_complete = !self.is_playing
            && !self.timeline.clips.is_empty()
            && self.playhead_ms == timeline_end_ms
            && self.playhead_ms > 0;
        let mut state_json = serde_json::json!({
            "playhead_ms": self.playhead_ms,
            "timeline_end_ms": timeline_end_ms,
            "is_playing": self.is_playing,
            "playback_complete": playback_complete,
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
            state_json["test_first_clip_scale_x"] =
                clip.scale_x.get_value_at(self.playhead_ms).into();
            state_json["test_first_clip_brightness"] =
                clip.brightness.get_value_at(self.playhead_ms).into();
            state_json["test_first_clip_speed"] = clip.speed.get_value_at(self.playhead_ms).into();
            state_json["test_first_clip_opacity"] =
                clip.opacity.get_value_at(self.playhead_ms).into();
            state_json["test_first_clip_reverse"] = clip.reverse.into();
            state_json["test_first_clip_frozen"] = clip.freeze_frame_ms.is_some().into();
            state_json["test_first_clip_stabilization"] = clip.stabilization_enabled.into();
            state_json["tracking_data_count"] = serde_json::json!(clip.tracking_data.len());
            state_json["motion_tracking_enabled"] = clip.motion_tracking_enabled.into();
            state_json["test_first_clip_motion_keyframes"] =
                serde_json::json!(clip.pos_x.keyframes.len().max(clip.pos_y.keyframes.len()));
            state_json["object_detection_enabled"] = clip.object_detection_enabled.into();
            state_json["detection_count"] = serde_json::json!(clip.detections.len());
            state_json["test_first_clip_detection_labels"] =
                serde_json::json!(clip.detections.len());
        }

        state_json
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
        self.drag_pointer_start_y = None;
        self.drag_clip_id = None;
        self.drag_start_track_index = None;
        self.drag_preview_times = None;
        self.drag_preview_track_index = None;
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

    fn write_recovery_checkpoint(&self) -> Result<Option<PathBuf>, String> {
        if !self.app_config.autosave_enabled {
            return Ok(None);
        }

        let path = self.recovery_checkpoint_path();
        let project = self.snapshot_project();
        project
            .save(&path)
            .map_err(|err| format!("Failed to save recovery checkpoint: {err}"))?;
        Ok(Some(path))
    }

    fn recovery_checkpoint_path(&self) -> PathBuf {
        self.current_project_path
            .as_ref()
            .map(AppConfig::recovery_checkpoint_path_for_project)
            .unwrap_or_else(AppConfig::untitled_recovery_checkpoint_path)
    }

    fn recovery_checkpoint_available(&self) -> bool {
        self.recovery_checkpoint_path().exists()
    }

    fn recovery_banner_message(&self) -> Option<&'static str> {
        self.recovery_checkpoint_available()
            .then_some("Recovery checkpoint available")
    }

    fn restore_recovery_checkpoint(&mut self) -> Result<(), String> {
        let path = self.recovery_checkpoint_path();
        let project = Project::load(&path).map_err(|err| err.to_string())?;
        self.reset_runtime_state(project, self.current_project_path.clone());
        Ok(())
    }
    fn draw_recovery_banner(&mut self, ui: &mut egui::Ui) {
        if self.recovery_banner_message().is_some() {
            ui.horizontal(|ui| {
                ui.label("Recovery checkpoint available");
                if ui.button("Restore Recovery Checkpoint").clicked() {
                    if let Err(err) = self.restore_recovery_checkpoint() {
                        log::error!("Failed to restore recovery checkpoint: {}", err);
                    }
                }
            });
        }
    }

    fn draw_export_dialog(&mut self, ctx: &egui::Context) {
        let exporting = self.active_export.is_some();
        if !self.show_export_dialog && !exporting {
            return;
        }

        if self.export_output_path.is_none() && !exporting {
            let settings = self.export_settings_for_selected_preset();
            let base_dir = self
                .export_destination_directory()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            self.export_output_path = Some(base_dir.join(self.export_default_file_name(&settings)));
        }

        let mut open = self.show_export_dialog || exporting;
        let mut request_browse = false;
        let mut request_start = false;
        let mut request_cancel = false;

        egui::Window::new("Export Video")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                if exporting {
                    if let Some(job) = &self.active_export {
                        ui.label(format!("Exporting to {}", job.output_path.display()));
                        ui.label(format!(
                            "{} / {} frames",
                            job.progress.completed_frames(),
                            job.progress.total_frames()
                        ));
                        ui.add(
                            egui::ProgressBar::new(job.progress.progress_fraction())
                                .show_percentage()
                                .desired_width(ui.available_width()),
                        );
                        let elapsed = job.started_at.elapsed();
                        if let Some(remaining) = job.progress.estimated_seconds_remaining(elapsed) {
                            ui.label(format!("ETA: {:.1}s", remaining.max(0.0)));
                        }
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Cancel Export").clicked() {
                            request_cancel = true;
                        }
                    });
                } else {
                    ui.label("Choose an export preset and output file.");
                    ui.separator();

                    egui::ComboBox::from_id_salt("export_preset")
                        .selected_text(
                            self.app_config
                                .export_presets
                                .get(self.export_preset_index)
                                .map(|preset| preset.name.as_str())
                                .unwrap_or("Default"),
                        )
                        .show_ui(ui, |ui| {
                            for (index, preset) in self.app_config.export_presets.iter().enumerate()
                            {
                                ui.selectable_value(
                                    &mut self.export_preset_index,
                                    index,
                                    format!(
                                        "{}  [{} {}x{} @ {:.2} fps]",
                                        preset.name,
                                        match preset.settings.format {
                                            ExportFormat::MP4 => "MP4",
                                            ExportFormat::WebM => "WebM",
                                            ExportFormat::MOV => "MOV",
                                            ExportFormat::MKV => "MKV",
                                        },
                                        preset.settings.width,
                                        preset.settings.height,
                                        preset.settings.fps_num as f32
                                            / preset.settings.fps_den.max(1) as f32,
                                    ),
                                );
                            }
                        });

                    if let Some(preset) =
                        self.app_config.export_presets.get(self.export_preset_index)
                    {
                        ui.small(&preset.description);
                        ui.label(format!(
                            "Format: {:?}, Codec: {:?}, Size: {}x{}, FPS: {:.2}, Bitrate: {} kbps",
                            preset.settings.format,
                            preset.settings.codec,
                            preset.settings.width,
                            preset.settings.height,
                            preset.settings.fps_num as f32 / preset.settings.fps_den.max(1) as f32,
                            preset.settings.bitrate_kbps,
                        ));
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Output:");
                        let path_label = self
                            .export_output_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "Not selected".to_string());
                        ui.monospace(path_label);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Browse...").clicked() {
                            request_browse = true;
                        }
                        let can_start = self.export_output_path.is_some();
                        if ui
                            .add_enabled(can_start, egui::Button::new("Start Export"))
                            .clicked()
                        {
                            request_start = true;
                        }
                    });
                    ui.small("The captured preview will be scaled to the selected preset size.");
                }

                if let Some(notification) = &self.export_notification {
                    ui.separator();
                    ui.label(notification);
                }
            });

        if request_browse {
            let settings = self.export_settings_for_selected_preset();
            self.export_output_path = self.pick_export_save_path(&settings);
        }
        if request_start {
            if let Err(err) = self.begin_export(ctx) {
                self.export_notification = Some(format!("Export failed: {err}"));
                log::error!("Failed to start export: {}", err);
            }
        }
        if request_cancel {
            self.request_active_export_cancel();
        }

        self.show_export_dialog = open && !self.active_export.is_some();
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
    fn pick_media_files(&mut self) -> Option<Vec<PathBuf>> {
        #[cfg(test)]
        if let Some(paths) = self.test_media_picker_paths.take() {
            return Some(paths);
        }

        if let Some(paths) = std::env::var_os("CLIPSTRIKE_TEST_MEDIA_PATHS").and_then(|value| {
            let paths: Vec<PathBuf> = value
                .to_string_lossy()
                .split(';')
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .collect();
            (!paths.is_empty()).then_some(paths)
        }) {
            return Some(paths);
        }

        rfd::FileDialog::new()
            .add_filter(
                "Video/Audio",
                &["mp4", "mkv", "mov", "wav", "mp3", "png", "jpg"],
            )
            .pick_files()
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_media_files(&mut self) -> Option<Vec<PathBuf>> {
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
    fn file_uri_from_path(path: &Path) -> Result<String, String> {
        url::Url::from_file_path(path)
            .map(|uri| uri.to_string())
            .map_err(|_| format!("Failed to convert path to file URI: {}", path.display()))
    }

    #[cfg(target_arch = "wasm32")]
    fn file_uri_from_path(path: &Path) -> Result<String, String> {
        let normalized = path.to_string_lossy().replace('\\', "/");
        if normalized.is_empty() {
            Err(format!(
                "Failed to convert path to file URI: {}",
                path.display()
            ))
        } else {
            Ok(format!("file:///{}", normalized))
        }
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

    fn title_svg_temp_path(title_id: usize) -> PathBuf {
        std::env::temp_dir().join(format!("title_{}.svg", title_id))
    }

    fn refresh_title_svg_from_path(
        &mut self,
        title_id: usize,
        svg_path: &Path,
    ) -> Result<Vec<usize>, String> {
        let svg_content = std::fs::read_to_string(svg_path)
            .map_err(|err| format!("Failed to read edited SVG {}: {err}", svg_path.display()))?;
        let title = self
            .project
            .titles
            .iter_mut()
            .find(|candidate| candidate.id == title_id)
            .ok_or_else(|| format!("Title {title_id} not found"))?;
        title.svg_content = svg_content;

        let svg_uri = Self::file_uri_from_path(svg_path)?;
        let affected_clip_ids = self
            .timeline
            .clips
            .iter()
            .filter(|clip| {
                self.project
                    .assets
                    .iter()
                    .any(|asset| asset.id == clip.asset_id && asset.uri == svg_uri)
            })
            .map(|clip| clip.id)
            .collect();

        Ok(affected_clip_ids)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn ensure_title_svg_edit_session(&mut self, title_id: usize) -> Result<(), String> {
        if self.title_svg_edit_sessions.contains_key(&title_id) {
            return Ok(());
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let watcher = notify::recommended_watcher(move |result| {
            let _ = tx.send(result);
        })
        .map_err(|err| format!("Failed to create SVG watcher: {err}"))?;

        self.title_svg_edit_sessions.insert(
            title_id,
            TitleEditSession {
                svg_path: Self::title_svg_temp_path(title_id),
                watcher,
                events: rx,
            },
        );

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn edit_title_in_inkscape(&mut self, title_id: usize) -> Result<(), String> {
        let title = self
            .project
            .titles
            .iter()
            .find(|candidate| candidate.id == title_id)
            .cloned()
            .ok_or_else(|| format!("Title {title_id} not found"))?;

        if self.app_config.inkscape_executable_path.trim().is_empty() {
            return Err("Configure an Inkscape executable path in Preferences first".to_string());
        }

        let svg_path = Self::title_svg_temp_path(title_id);
        let existing_session = self.title_svg_edit_sessions.contains_key(&title_id);
        if let Some(parent) = svg_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "Failed to create title edit directory {}: {err}",
                    parent.display()
                )
            })?;
        }
        std::fs::write(&svg_path, &title.svg_content).map_err(|err| {
            format!(
                "Failed to write SVG for external editing {}: {err}",
                svg_path.display()
            )
        })?;

        self.ensure_title_svg_edit_session(title_id)?;

        let bridge = InkscapeBridge::new(PathBuf::from(
            self.app_config.inkscape_executable_path.clone(),
        ));
        let _ = bridge.edit_svg(&svg_path)?;

        if !existing_session {
            if let Some(session) = self.title_svg_edit_sessions.get_mut(&title_id) {
                session.svg_path = svg_path;
                session
                    .watcher
                    .watch(&session.svg_path, RecursiveMode::NonRecursive)
                    .map_err(|err| {
                        format!(
                            "Failed to watch edited SVG {}: {err}",
                            session.svg_path.display()
                        )
                    })?;
            }
        }

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_title_svg_edit_sessions(&mut self, ctx: &egui::Context) {
        let mut changed_titles = Vec::new();

        for (title_id, session) in &mut self.title_svg_edit_sessions {
            while let Ok(event) = session.events.try_recv() {
                let Ok(event) = event else {
                    continue;
                };
                if event.paths.iter().any(|path| path == &session.svg_path) {
                    changed_titles.push((*title_id, session.svg_path.clone()));
                    break;
                }
            }
        }

        let needs_repaint = !changed_titles.is_empty();
        for (title_id, svg_path) in changed_titles {
            match self.refresh_title_svg_from_path(title_id, &svg_path) {
                Ok(affected_clip_ids) => {
                    if let Some(engine_manager) = &mut self.engine_manager {
                        for clip_id in affected_clip_ids {
                            let _ = engine_manager.remove_clip_engine(clip_id);
                        }
                    }
                }
                Err(err) => log::warn!(
                    "Failed to refresh title SVG {}: {}",
                    svg_path.display(),
                    err
                ),
            }
        }

        if needs_repaint {
            ctx.request_repaint();
        }
    }

    fn import_media_path(&mut self, path: PathBuf) {
        let uri = match Self::file_uri_from_path(&path) {
            Ok(uri) => uri,
            Err(err) => {
                log::error!("{}", err);
                return;
            }
        };
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Imported Media")
            .to_string();
        let suppress_import_warnings = std::env::var_os("CLIPSTRIKE_TEST_MEDIA_PATHS").is_some();
        let duration_ms = match probe_metadata_sync(&uri) {
            Ok(metadata) => metadata
                .duration
                .map(|duration: std::time::Duration| {
                    duration.as_millis().min(u64::MAX as u128) as u64
                })
                .unwrap_or(0),
            Err(err) => {
                if suppress_import_warnings {
                    log::debug!("Failed to probe media metadata for {}: {}", uri, err);
                } else {
                    log::warn!("Failed to probe media metadata for {}: {}", uri, err);
                }
                0
            }
        };
        let audio_levels = match extract_waveform_sync(&uri, 96) {
            Ok(levels) => levels,
            Err(err) => {
                if suppress_import_warnings {
                    log::debug!("Failed to extract waveform for {}: {}", uri, err);
                } else {
                    log::warn!("Failed to extract waveform for {}: {}", uri, err);
                }
                Vec::new()
            }
        };

        let asset_id = rand::random::<usize>();
        log::info!(
            "Imported media asset id={} name={} duration_ms={} waveform_bins={}",
            asset_id,
            name,
            duration_ms,
            audio_levels.len()
        );
        self.project.assets.push(MediaAsset {
            id: asset_id,
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
        log::debug!(
            "Applied track mutation: project_clips={} timeline_clips={} selected_clip={:?}",
            self.project.clips.len(),
            self.timeline.clips.len(),
            self.selected_clip_id
        );
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
        if let Err(err) = self.write_recovery_checkpoint() {
            log::error!("Failed to save recovery checkpoint: {}", err);
        }
        Ok(())
    }

    fn frame_duration_ms(&self) -> i64 {
        (1000.0 / self.project.framerate.max(1.0)).round() as i64
    }

    fn set_playing_state(&mut self, playing: bool) {
        self.is_playing = playing;
        self.seek_playhead();
        if let Some(audio) = &self.audio_engine {
            audio.set_playing(self.is_playing);
        }
    }

    fn toggle_playback(&mut self) {
        self.set_playing_state(!self.is_playing);
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

    fn step_playback(&mut self, delta_ms: i64) {
        let step = advance_playhead(self.playhead_ms, delta_ms, self.timeline_end_ms());
        self.playhead_ms = step.playhead_ms;
        if step.should_stop {
            self.set_playing_state(false);
        }
    }

    fn handle_clip_eos(&mut self, clip_id: usize) {
        let Some(step) = clip_eos_step(&self.timeline.clips, clip_id, self.timeline_end_ms())
        else {
            return;
        };

        self.playhead_ms = step.playhead_ms;
        if step.should_stop {
            self.set_playing_state(false);
        } else {
            self.seek_playhead();
        }
    }

    fn process_engine_events(&mut self) {
        let Some(engine_manager) = &mut self.engine_manager else {
            return;
        };

        for event in engine_manager.drain_events() {
            match event {
                clipstrike_media::engine_manager::EngineEvent::EndOfStream { clip_id } => {
                    self.handle_clip_eos(clip_id);
                }
            }
        }
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

    fn cached_value<T, F>(slot: &OnceCell<T>, init: F) -> &T
    where
        F: FnOnce() -> T,
    {
        slot.get_or_init(init)
    }

    fn media_asset_has_audio(asset: &MediaAsset) -> bool {
        let asset_ext = asset.uri.rsplit('.').next().unwrap_or("").to_lowercase();

        !asset.audio_levels.is_empty()
            || matches!(
                asset_ext.as_str(),
                "mp4" | "mkv" | "mov" | "webm" | "avi" | "wav" | "mp3" | "aac" | "flac"
            )
    }

    fn playback_audio_samples(
        samples: std::sync::Arc<Vec<f32>>,
    ) -> Option<std::sync::Arc<Vec<f32>>> {
        (!samples.is_empty()).then_some(samples)
    }

    fn fx_sampler(&self, device: &wgpu::Device) -> &wgpu::Sampler {
        Self::cached_value(&self.fx_sampler, || {
            device.create_sampler(&wgpu::SamplerDescriptor::default())
        })
    }

    fn preview_texture_source_kind(frame: &FrameTexture) -> FrameSourceKind {
        clipstrike_render::gpu::frame_source_kind(
            frame.external_texture_id,
            frame.has_native_preview(),
        )
    }

    fn clip_has_active_preview_fx(clip: &Clip, playhead_ms: i64) -> bool {
        clip.brightness.get_value_at(playhead_ms) != 1.0
            || clip.contrast.get_value_at(playhead_ms) != 1.0
            || clip.saturation.get_value_at(playhead_ms) != 1.0
            || clip.grayscale.get_value_at(playhead_ms) != 0.0
            || clip.blur_radius.get_value_at(playhead_ms) != 0.0
            || clip.opacity.get_value_at(playhead_ms) != 1.0
            || clip.pixelate_size.get_value_at(playhead_ms) != 1.0
            || clip.gamma.get_value_at(playhead_ms) != 1.0
            || clip.hue.get_value_at(playhead_ms) != 0.0
            || clip.color_tint_r.get_value_at(playhead_ms) != 1.0
            || clip.color_tint_g.get_value_at(playhead_ms) != 1.0
            || clip.color_tint_b.get_value_at(playhead_ms) != 1.0
            || clip.chroma_key_threshold.get_value_at(playhead_ms) != 0.0
            || clip.chroma_key_fuzziness.get_value_at(playhead_ms) != 0.1
            || clip.chroma_key_r.get_value_at(playhead_ms) != 0.0
            || clip.chroma_key_g.get_value_at(playhead_ms) != 1.0
            || clip.chroma_key_b.get_value_at(playhead_ms) != 0.0
            || clip.lut_enabled.get_value_at(playhead_ms) != 0.0
            || clip.stabilization_enabled
            || clip.motion_tracking_enabled
            || !clip.effects.is_empty()
    }

    fn timeline_time_from_ruler_position(
        origin_x: f32,
        pointer_pos: egui::Pos2,
        zoom_level: f32,
    ) -> i64 {
        let pixels_per_ms = (zoom_level * 0.1).max(f32::EPSILON);
        let local_x = (pointer_pos.x - origin_x).max(0.0);
        (local_x / pixels_per_ms) as i64
    }

    fn clip_split_time_from_pointer(
        clip_start_ms: i64,
        clip_rect_left: f32,
        pointer_x: f32,
        pixels_per_ms: f32,
    ) -> i64 {
        let local_x = (pointer_x - clip_rect_left).max(0.0);
        clip_start_ms.saturating_add((local_x / pixels_per_ms.max(f32::EPSILON)) as i64)
    }

    fn preview_texture_should_bypass_fx(
        clip: &Clip,
        playhead_ms: i64,
        source_kind: FrameSourceKind,
    ) -> bool {
        matches!(
            source_kind,
            FrameSourceKind::ExternalTexture(_) | FrameSourceKind::ImportedNativeTexture
        ) && !Self::clip_has_active_preview_fx(clip, playhead_ms)
    }

    fn stabilization_translation(
        clip: &Clip,
        playhead_ms: i64,
        frame_width: u32,
        frame_height: u32,
    ) -> Vec3 {
        if !clip.stabilization_enabled {
            return Vec3::ZERO;
        }

        let Some(track) = clip.tracking_data.first() else {
            return Vec3::ZERO;
        };
        let Some(anchor) = track.points.first() else {
            return Vec3::ZERO;
        };

        let current_point = track
            .points
            .iter()
            .rev()
            .find(|point| point.time_ms <= playhead_ms)
            .unwrap_or(anchor);

        let strength = clip
            .stabilization_strength
            .get_value_at(playhead_ms)
            .clamp(0.0, 1.0);

        Vec3::new(
            (anchor.x - current_point.x) * frame_width as f32 * strength,
            (anchor.y - current_point.y) * frame_height as f32 * strength,
            0.0,
        )
    }

    fn preview_transform_for_clip(
        clip: &Clip,
        playhead_ms: i64,
        frame_width: u32,
        frame_height: u32,
    ) -> [f32; 16] {
        let translation =
            Vec3::new(
                clip.pos_x.get_value_at(playhead_ms),
                clip.pos_y.get_value_at(playhead_ms),
                0.0,
            ) + Self::stabilization_translation(clip, playhead_ms, frame_width, frame_height);
        let scale = Vec3::new(
            clip.scale_x.get_value_at(playhead_ms),
            clip.scale_y.get_value_at(playhead_ms),
            1.0,
        );

        clipstrike_render::fx::KeyframeEngine::calculate_transform(scale, translation)
            .to_cols_array()
    }

    fn preview_texture_id(&self, clip: &Clip, frame: &FrameTexture) -> Option<egui::TextureId> {
        let source_kind = Self::preview_texture_source_kind(frame);
        let texture_id = frame.egui_id?;

        if Self::preview_texture_should_bypass_fx(clip, self.playhead_ms, source_kind) {
            return Some(texture_id);
        }
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
            pixelate_size: clip.pixelate_size.get_value_at(self.playhead_ms),
            color_tint: [
                clip.color_tint_r.get_value_at(self.playhead_ms),
                clip.color_tint_g.get_value_at(self.playhead_ms),
                clip.color_tint_b.get_value_at(self.playhead_ms),
            ],
            alpha: clip.opacity.get_value_at(self.playhead_ms),
            transform: Self::preview_transform_for_clip(clip, self.playhead_ms, 1920, 1080),
            ..Default::default()
        };

        let fx_sampler = self.fx_sampler(device);
        fx_pipeline.run(
            device,
            queue,
            frame.preview_view(),
            fx_output_view,
            None,
            fx_sampler,
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
    fn save_rgba_buffer_to_path(
        path: &std::path::Path,
        pixels: &mut Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        image::save_buffer(
            path,
            pixels.as_slice(),
            width,
            height,
            image::ColorType::Rgba8,
        )
        .map_err(|err| format!("Failed to save frame: {err}"))
    }

    fn save_current_frame_to_path(&mut self, path: &std::path::Path) -> Result<(), String> {
        let texture_pool = self
            .texture_pool
            .as_ref()
            .ok_or_else(|| "WGPU state missing".to_string())?;
        let queue = self
            .queue
            .as_ref()
            .ok_or_else(|| "WGPU state missing".to_string())?;
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| "WGPU state missing".to_string())?;
        let fx_output_texture = self
            ._fx_output_texture
            .as_ref()
            .ok_or_else(|| "WGPU state missing".to_string())?;

        let (frame_index, width, height) = {
            let frame = texture_pool
                .get_frames()
                .iter()
                .find(|texture| texture.ready)
                .ok_or_else(|| "No frame available to export".to_string())?;
            (frame.index, texture_pool.width, texture_pool.height)
        };

        self.save_frame_buffer.clear();
        if self.selected_clip_id.is_some() {
            read_texture_rgba_into(
                device,
                queue,
                fx_output_texture,
                width,
                height,
                &mut self.save_frame_buffer,
            )?;
        } else {
            let frame = texture_pool
                .get_frames()
                .iter()
                .find(|texture| texture.ready && texture.index == frame_index)
                .ok_or_else(|| "No frame available to export".to_string())?;
            read_texture_rgba_into(
                device,
                queue,
                &frame.texture,
                width,
                height,
                &mut self.save_frame_buffer,
            )?;
        }

        Self::save_rgba_buffer_to_path(path, &mut self.save_frame_buffer, width, height)
    }

    fn export_default_file_name(&self, settings: &ExportSettings) -> String {
        let stem = self
            .project
            .name
            .trim()
            .chars()
            .map(|ch| match ch {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => ch,
            })
            .collect::<String>();
        let base = if stem.is_empty() {
            "export".to_string()
        } else {
            stem
        };
        format!("{}.{}", base, Self::export_file_extension(settings.format))
    }

    fn export_file_extension(format: ExportFormat) -> &'static str {
        match format {
            ExportFormat::MP4 => "mp4",
            ExportFormat::WebM => "webm",
            ExportFormat::MOV => "mov",
            ExportFormat::MKV => "mkv",
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn pick_export_save_path(&self, settings: &ExportSettings) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new();
        dialog = dialog.add_filter("Video", &["mp4", "mov", "mkv", "webm"]);
        if let Some(dir) = &self.app_config.last_export_directory {
            dialog = dialog.set_directory(dir);
        } else if let Some(project_path) = &self.current_project_path {
            if let Some(parent) = project_path.parent() {
                dialog = dialog.set_directory(parent);
            }
        }
        dialog
            .set_file_name(self.export_default_file_name(settings))
            .save_file()
    }

    #[cfg(target_arch = "wasm32")]
    fn pick_export_save_path(&self, _settings: &ExportSettings) -> Option<PathBuf> {
        log::warn!("Export dialog is not available on web builds.");
        None
    }

    fn capture_composited_preview_rgba_buffer(&mut self) -> Result<(u32, u32), String> {
        let texture_pool = self
            .texture_pool
            .as_ref()
            .ok_or_else(|| "WGPU state missing".to_string())?;
        let queue = self
            .queue
            .as_ref()
            .ok_or_else(|| "WGPU state missing".to_string())?;
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| "WGPU state missing".to_string())?;
        let accumulator_texture = self
            ._accumulator_texture
            .as_ref()
            .ok_or_else(|| "WGPU state missing".to_string())?;

        let width = texture_pool.width;
        let height = texture_pool.height;
        self.save_frame_buffer.clear();
        read_texture_rgba_into(
            device,
            queue,
            accumulator_texture,
            width,
            height,
            &mut self.save_frame_buffer,
        )?;
        Ok((width, height))
    }

    fn resize_rgba_buffer(
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

    fn export_settings_for_selected_preset(&self) -> ExportSettings {
        self.app_config
            .export_presets
            .get(self.export_preset_index)
            .map(|preset| preset.settings.clone())
            .unwrap_or_default()
    }

    fn export_destination_directory(&self) -> Option<PathBuf> {
        self.export_output_path
            .as_ref()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| self.app_config.last_export_directory.clone())
    }

    fn remember_export_directory(&mut self, path: &Path) {
        if let Some(parent) = path.parent() {
            self.app_config.last_export_directory = Some(parent.to_path_buf());
            self.persist_app_config();
        }
    }

    fn begin_export(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.active_export.is_some() {
            return Err("An export is already running".to_string());
        }

        let settings = self.export_settings_for_selected_preset();
        let output_path = if let Some(path) = self.export_output_path.clone() {
            path
        } else {
            self.pick_export_save_path(&settings)
                .ok_or_else(|| "Export cancelled before selecting an output file".to_string())?
        };
        self.export_output_path = Some(output_path.clone());
        self.remember_export_directory(&output_path);

        let frame_duration_ms = ((1000.0 * settings.fps_den.max(1) as f64)
            / settings.fps_num.max(1) as f64)
            .round()
            .max(1.0) as i64;
        let total_frames =
            export_frame_count_for_duration_ms(self.timeline_end_ms(), frame_duration_ms);
        let exporter = Exporter::new(&output_path, &settings)?;
        exporter.start()?;

        let restore_playhead_ms = self.playhead_ms;
        let restore_is_playing = self.is_playing;
        self.set_playing_state(false);
        self.playhead_ms = 0;
        self.seek_playhead();

        self.active_export = Some(ActiveExportJob {
            output_path,
            exporter,
            progress: ExportProgress::new(total_frames),
            settings,
            next_frame_ms: 0,
            frame_duration_ms,
            pending_capture: false,
            capture_ready: false,
            started_at: Instant::now(),
            restore_playhead_ms,
            restore_is_playing,
        });
        self.export_notification = Some("Export started".to_string());
        self.show_export_dialog = false;
        ctx.request_repaint();
        Ok(())
    }

    fn request_active_export_cancel(&mut self) {
        if let Some(job) = &mut self.active_export {
            job.progress.request_cancel();
            self.export_notification = Some("Export cancel requested".to_string());
        }
    }

    fn restore_after_export(&mut self, restore_playhead_ms: i64, restore_is_playing: bool) {
        self.playhead_ms = restore_playhead_ms;
        self.seek_playhead();
        self.set_playing_state(restore_is_playing);
    }

    fn finalize_export_job(&mut self, job: ActiveExportJob, outcome: Result<(), String>) {
        let _ = job.exporter.stop();
        let restore_playhead_ms = job.restore_playhead_ms;
        let restore_is_playing = job.restore_is_playing;
        let output_path = job.output_path.clone();
        self.active_export = None;
        self.show_export_dialog = false;
        self.restore_after_export(restore_playhead_ms, restore_is_playing);

        match outcome {
            Ok(()) => {
                self.export_notification =
                    Some(format!("Export complete: {}", output_path.display()));
            }
            Err(err) => {
                let _ = std::fs::remove_file(&output_path);
                self.export_notification = Some(format!("Export failed: {err}"));
            }
        }
    }

    fn process_active_export_seek(&mut self, ctx: &egui::Context) {
        let timeline_end_ms = self.timeline_end_ms();
        let mut next_seek_ms = None;
        let mut cancel_requested = false;
        if let Some(job) = &mut self.active_export {
            cancel_requested = job.progress.cancel_requested();
            if !job.pending_capture {
                next_seek_ms = Some(job.next_frame_ms.min(timeline_end_ms));
                job.pending_capture = true;
                job.capture_ready = false;
            }
        }

        if cancel_requested {
            let job = self.active_export.take().expect("export job should exist");
            self.finalize_export_job(job, Err("Export canceled".to_string()));
            return;
        }

        if let Some(next_seek_ms) = next_seek_ms {
            self.playhead_ms = next_seek_ms;
            self.seek_playhead();
            ctx.request_repaint();
        }
    }

    fn process_active_export_capture(&mut self, ctx: &egui::Context) {
        let Some(mut job) = self.active_export.take() else {
            return;
        };

        if job.progress.cancel_requested() {
            self.finalize_export_job(job, Err("Export canceled".to_string()));
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
            job.next_frame_ms = job.next_frame_ms.saturating_add(job.frame_duration_ms);
            Ok(())
        })();

        if let Err(err) = capture_result {
            self.finalize_export_job(job, Err(err));
            return;
        }

        if job.progress.is_finished() {
            let finalize_result = (|| {
                job.exporter.end_of_stream()?;
                Ok(())
            })();
            self.finalize_export_job(job, finalize_result);
        } else {
            self.active_export = Some(job);
            ctx.request_repaint();
        }
    }

    fn draw_clip_waveform(
        ui: &mut egui::Ui,
        rect: egui::Rect,
        audio_levels: &[f32],
        clip_in_ms: i64,
        clip_out_ms: i64,
        asset_duration_ms: u64,
    ) {
        if audio_levels.is_empty() {
            return;
        }

        let mut mesh = egui::Mesh::default();
        let color = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100);
        let width = rect.width();
        let height = rect.height();
        let mid_y = rect.center().y;

        let asset_duration_ms = asset_duration_ms.max(1) as f64;
        let clip_in_f = clip_in_ms as f64;
        let clip_out_f = clip_out_ms as f64;
        let clip_duration_f = (clip_out_f - clip_in_f).max(1.0);

        for i in 0..(width as usize) {
            let t = i as f64 / width as f64;
            let timeline_ms = clip_in_f + t * clip_duration_f;
            let asset_t = timeline_ms / asset_duration_ms;

            if asset_t < 0.0 || asset_t > 1.0 {
                continue;
            }

            let bin_idx = (asset_t * (audio_levels.len() as f64)) as usize;
            if let Some(&level) = audio_levels.get(bin_idx) {
                let amplitude = (level * height * 0.4).max(1.0);
                let x = rect.left() + i as f32;
                mesh.add_colored_rect(
                    egui::Rect::from_min_max(
                        egui::pos2(x, mid_y - amplitude),
                        egui::pos2(x + 1.0, mid_y + amplitude),
                    ),
                    color,
                );
            }
        }
        ui.painter().add(mesh);
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

    fn playback_clips_for_playhead(&self) -> (Vec<Clip>, Vec<Clip>) {
        let active_clips = self.timeline.get_clips_at_time(self.playhead_ms);
        let prewarm_window_ms = self.frame_duration_ms().saturating_mul(8).max(250);
        let prewarm_end_ms = self.playhead_ms.saturating_add(prewarm_window_ms);
        let mut prewarm_clips = Vec::new();

        for track in &self.project.tracks {
            if let Some(next_clip) = self
                .timeline
                .clips
                .iter()
                .filter(|clip| {
                    clip.track_id == track.id
                        && clip.start > self.playhead_ms
                        && clip.start <= prewarm_end_ms
                })
                .min_by_key(|clip| (clip.start, clip.id))
            {
                prewarm_clips.push(next_clip.clone());
            }
        }

        prewarm_clips.sort_by_key(|clip| (clip.start, clip.track_id, clip.id));
        (active_clips, prewarm_clips)
    }

    fn visible_tracks_in_render_order(&self) -> impl Iterator<Item = &Track> {
        let max_track_id = self
            .project
            .tracks
            .iter()
            .map(|track| track.id)
            .max()
            .unwrap_or(0);

        (1..=max_track_id).filter_map(move |track_id| {
            self.project
                .tracks
                .iter()
                .find(|track| track.visible && track.id == track_id)
        })
    }

    fn clips_for_track_at_playhead(&self, track_id: usize) -> impl Iterator<Item = &Clip> {
        self.timeline.clips.iter().filter(move |clip| {
            clip.track_id == track_id
                && clip.start <= self.playhead_ms
                && self.playhead_ms < clip.end
        })
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

    fn sync_engines_for_playhead(
        &mut self,
        active_clips: &[Clip],
        prewarm_clips: &[Clip],
        seek_now: bool,
    ) {
        let engine_manager = match &mut self.engine_manager {
            Some(em) => em,
            None => return,
        };
        let mut sync_ids: Vec<usize> = active_clips
            .iter()
            .chain(prewarm_clips.iter())
            .map(|clip| clip.id)
            .collect();
        sync_ids.sort_unstable();
        sync_ids.dedup();
        log::debug!(
            "sync_engines_for_playhead: playhead_ms={} seek_now={} active_clip_ids={:?} prewarm_clip_ids={:?}",
            self.playhead_ms,
            seek_now,
            active_clips.iter().map(|clip| clip.id).collect::<Vec<_>>(),
            prewarm_clips.iter().map(|clip| clip.id).collect::<Vec<_>>()
        );
        engine_manager.remove_unused(&sync_ids);

        let mut audio_sources = Vec::new();

        let mut created_engine = false;
        for clip in active_clips.iter().chain(prewarm_clips.iter()) {
            if let Some(asset) = self.project.assets.iter().find(|a| a.id == clip.asset_id) {
                if asset.duration_ms == 0 {
                    log::debug!(
                        "sync_engines_for_playhead: skipping zero-duration asset for clip_id={} asset_id={} uri={}",
                        clip.id,
                        asset.id,
                        asset.uri
                    );
                } else {
                    log::debug!(
                        "sync_engines_for_playhead: clip_id={} asset_id={} uri={} video_enabled={} audio_enabled={}",
                        clip.id,
                        asset.id,
                        asset.uri,
                        clip.video_enabled,
                        clip.audio_enabled
                    );
                    match engine_manager.sync_clip(clip, &asset.uri) {
                        Ok(created) => {
                            if created {
                                created_engine = true;
                                if prewarm_clips
                                    .iter()
                                    .any(|warm_clip| warm_clip.id == clip.id)
                                {
                                    let _ = engine_manager
                                        .seek_clips(std::slice::from_ref(clip), clip.start);
                                }
                            }
                        }
                        Err(err) => log::error!("Failed to sync clip {}: {}", clip.id, err),
                    }
                }
            } else {
                log::warn!(
                    "sync_engines_for_playhead: missing asset for clip_id={} asset_id={}",
                    clip.id,
                    clip.asset_id
                );
            }
            let volume = if self.project.is_track_audible(clip.track_id) {
                (clip.volume.get_value_at(self.playhead_ms) * self.master_volume).clamp(0.0, 2.0)
                    as f64
            } else {
                0.0
            };

            engine_manager.set_clip_volume(clip.id, volume);

            // Synchronize Audio Engine (Task 12.5, 12.1)
            if self.project.is_track_audible(clip.track_id) && clip.audio_enabled {
                audio_sources.push(clipstrike_audio::AudioSource {
                    clip_id: clip.id,
                    samples: Self::playback_audio_samples(
                        engine_manager.get_audio_samples(clip.id),
                    ),
                    start_ms: clip.start,
                    end_ms: clip.end,
                    volume: clip.volume.clone(),
                });
            }
        }

        if let Some(audio) = &self.audio_engine {
            audio.set_sources(audio_sources);
        }

        if seek_now {
            for clip in prewarm_clips {
                let _ = engine_manager.seek_clips(std::slice::from_ref(clip), clip.start);
            }
        }

        if seek_now || created_engine {
            engine_manager.seek_clips(active_clips, self.playhead_ms);
        }

        if self.is_playing {
            let active_ids: Vec<usize> = active_clips.iter().map(|clip| clip.id).collect();
            engine_manager.play_clips(&active_ids);
        } else {
            engine_manager.pause_all();
        }
    }

    fn interactive_seek_playhead_active_only(&mut self) {
        let (all_active_clips, _) = self.active_clips_for_playhead();
        self.sync_engines_for_playhead(&all_active_clips, &[], false);
        if let Some(engine_manager) = &self.engine_manager {
            engine_manager.seek_clips_interactive(&all_active_clips, self.playhead_ms);
        }
        if let Some(audio) = &self.audio_engine {
            audio.set_playhead(self.playhead_ms);
        }
    }

    fn seek_playhead(&mut self) {
        let (all_active_clips, prewarm_clips) = self.playback_clips_for_playhead();
        self.sync_engines_for_playhead(&all_active_clips, &prewarm_clips, true);
        if let Some(audio) = &self.audio_engine {
            audio.set_playhead(self.playhead_ms);
        }
    }

    fn seek_playhead_active_only(&mut self) {
        let (all_active_clips, _) = self.active_clips_for_playhead();
        self.sync_engines_for_playhead(&all_active_clips, &[], true);
        if let Some(audio) = &self.audio_engine {
            audio.set_playhead(self.playhead_ms);
        }
    }

    fn scrub_playhead(&mut self, target_ms: i64) {
        let target_ms = target_ms.clamp(0, self.timeline_end_ms());
        if target_ms == self.playhead_ms {
            return;
        }

        self.playhead_ms = target_ms;
        if self.is_playing {
            self.interactive_seek_playhead_active_only();
        } else {
            self.seek_playhead();
        }
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
                            TitleTemplate::EndCredits | TitleTemplate::Subtitle | TitleTemplate::Watermark => {
                                (0.5, 0.5, "middle")
                            }
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
                    self.undo_stack.push(
                        Box::new(clipstrike_core::commands::AddTrackCommand { track_id: None }),
                        &mut self.project,
                        &mut self.timeline,
                    );
                } else if cmd == "UNDO" {
                    self.undo_stack.undo(&mut self.project, &mut self.timeline);
                } else if cmd == "REDO" {
                    self.undo_stack.redo(&mut self.project, &mut self.timeline);
                } else if cmd == "ADD_TEST_ASSET" {
                    self.project
                        .assets
                        .push(clipstrike_core::project::MediaAsset {
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
                    self.project
                        .assets
                        .push(clipstrike_core::project::MediaAsset {
                            id: 1001,
                            name: "Forest.mp4".to_string(),
                            uri: "file:///forest.mp4".to_string(),
                            duration_ms: 5000,
                            audio_levels: vec![],
                        });
                    self.project
                        .assets
                        .push(clipstrike_core::project::MediaAsset {
                            id: 1002,
                            name: "Music.mp3".to_string(),
                            uri: "file:///music.mp3".to_string(),
                            duration_ms: 120000,
                            audio_levels: vec![],
                        });
                    self.project
                        .assets
                        .push(clipstrike_core::project::MediaAsset {
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
                                if let Some(pos) = self
                                    .timeline
                                    .clips
                                    .iter_mut()
                                    .position(|c| c.id == first_id)
                                {
                                    let clip = &mut self.timeline.clips[pos];
                                    property_mut(clip, property).insert_keyframe(
                                        self.playhead_ms,
                                        value,
                                        clipstrike_core::keyframes::InterpolationMode::Linear,
                                    );
                                }
                            }
                        }
                    }
                } else if cmd.starts_with("SET_CLIP_REVERSE:") {
                    let val = cmd["SET_CLIP_REVERSE:".len()..]
                        .parse::<bool>()
                        .unwrap_or(false);
                    if let Some(first_id) = self.timeline.clips.first().map(|c| c.id) {
                        if let Some(pos) = self
                            .timeline
                            .clips
                            .iter_mut()
                            .position(|c| c.id == first_id)
                        {
                            self.timeline.clips[pos].reverse = val;
                        }
                    }
                } else if cmd == "FREEZE_CLIP" {
                    if let Some(first_id) = self.timeline.clips.first().map(|c| c.id) {
                        if let Some(pos) = self
                            .timeline
                            .clips
                            .iter_mut()
                            .position(|c| c.id == first_id)
                        {
                            let current_source_ms =
                                self.timeline.clips[pos].source_time_at(self.playhead_ms);
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
                    let val = cmd["SET_ADVANCED_VIEW:".len()..]
                        .parse::<bool>()
                        .unwrap_or(true);
                    self.app_config.advanced_view = val;
                    self.persist_app_config();
                } else if cmd.starts_with("SET_FULLSCREEN:") {
                    let val = cmd["SET_FULLSCREEN:".len()..]
                        .parse::<bool>()
                        .unwrap_or(false);
                    self.app_config.fullscreen = val;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(val));
                    self.persist_app_config();
                } else if cmd == "OPEN_PREFERENCES" {
                    self.show_preferences_dialog = true;
                } else if cmd.starts_with("ADD_CLIP_TO_TRACK:") {
                    let parts: Vec<&str> = cmd["ADD_CLIP_TO_TRACK:".len()..].split(':').collect();
                    if parts.len() == 2 {
                        if let (Ok(track_id), Ok(start_ms)) =
                            (parts[0].parse::<usize>(), parts[1].parse::<i64>())
                        {
                            let mut clip = Clip::new(
                                rand::random::<usize>(),
                                999,
                                track_id,
                                start_ms,
                                start_ms + 5000,
                                0,
                                5000,
                            );
                            clip.audio_enabled = false;
                            clip.video_enabled = true;
                            clip.reverse = false;
                            clip.freeze_frame_ms = None;
                            clip.stabilization_enabled = false;
                            clip.motion_tracking_enabled = false;
                            clip.object_detection_enabled = false;
                            self.undo_stack.push(
                                Box::new(AddClipCommand { clip }),
                                &mut self.project,
                                &mut self.timeline,
                            );
                        }
                    }
                } else if cmd == "ADD_IMAGE_SEQUENCE" {
                    self.project
                        .assets
                        .push(clipstrike_core::project::MediaAsset {
                            id: 1004,
                            name: "Sequence_%04d.png".to_string(),
                            uri: "file:///seq_%04d.png".to_string(),
                            duration_ms: 10000,
                            audio_levels: vec![],
                        });
                } else if cmd.starts_with("SET_CLIP_STABILIZATION:") {
                    let val = cmd["SET_CLIP_STABILIZATION:".len()..]
                        .parse::<bool>()
                        .unwrap_or(false);
                    if let Some(first_id) = self.timeline.clips.first().map(|c| c.id) {
                        if let Some(pos) = self
                            .timeline
                            .clips
                            .iter_mut()
                            .position(|c| c.id == first_id)
                        {
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
                        match self.create_title_clip(title_id, track_id, self.playhead_ms, 5_000) {
                            Ok(clip) => {
                                self.undo_stack.push(
                                    Box::new(AddClipCommand { clip }),
                                    &mut self.project,
                                    &mut self.timeline,
                                );
                            }
                            Err(err) => {
                                log::error!(
                                    "Failed to add title {} to timeline from test hook: {}",
                                    title_id,
                                    err
                                );
                            }
                        }
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
                    if let Ok(project) =
                        serde_json::from_str::<clipstrike_core::project::Project>(json)
                    {
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
            let was_playing = self.is_playing;

            // Try to derive playhead from GStreamer's actual position to avoid drift.
            let mut engine_synced = false;
            if let Some(engine_manager) = &self.engine_manager {
                // Find the primary active clip at the current playhead
                if let Some(primary_clip) = self
                    .timeline
                    .clips
                    .iter()
                    .find(|clip| clip.start <= self.playhead_ms && self.playhead_ms < clip.end)
                {
                    if let Some(source_pos_ms) =
                        engine_manager.query_clip_position_ms(primary_clip.id)
                    {
                        let engine_timeline_ms =
                            primary_clip.timeline_time_from_source(source_pos_ms);
                        let timeline_end = self.timeline_end_ms();
                        let clamped = engine_timeline_ms.clamp(0, timeline_end);
                        if clamped >= timeline_end {
                            self.playhead_ms = timeline_end;
                            self.set_playing_state(false);
                        } else {
                            self.playhead_ms = clamped;
                        }
                        engine_synced = true;
                    }
                }
            }

            // Fallback: advance with UI delta if no engine position available
            if !engine_synced {
                self.step_playback((dt * 1000.0) as i64);
            }

            // Task 6.7: Looped playback
            if was_playing && self.loop_enabled && self.playhead_ms >= self.loop_end_ms {
                self.playhead_ms = self.loop_start_ms;
                self.set_playing_state(true);
            }

            ctx.request_repaint(); // Keep the loop running
        }

        #[cfg(not(target_arch = "wasm32"))]
        self.poll_title_svg_edit_sessions(ctx);

        self.process_engine_events();

        // Sync engines with active clips at playhead
        let (all_active_clips, prewarm_clips) = self.playback_clips_for_playhead();
        let frame_duration_ms = self.frame_duration_ms();
        let asset_duration_by_id: HashMap<usize, u64> = self
            .project
            .assets
            .iter()
            .map(|asset| (asset.id, asset.duration_ms))
            .collect();
        self.sync_engines_for_playhead(&all_active_clips, &prewarm_clips, !self.is_playing);

        let mut request_new_project = false;
        let mut request_open_project = false;
        let mut request_save_project = false;
        let mut request_save_as_project = false;
        let mut request_open_recent_project: Option<PathBuf> = None;
        let mut request_toggle_fullscreen = false;

        if shortcut_consumed(ctx, &self.app_config.shortcuts.play_pause) {
            self.toggle_playback();
        }
        if shortcut_consumed(ctx, &self.app_config.shortcuts.undo) {
            self.undo_stack.undo(&mut self.project, &mut self.timeline);
        }
        if shortcut_consumed(ctx, &self.app_config.shortcuts.redo) {
            self.undo_stack.redo(&mut self.project, &mut self.timeline);
        }
        if shortcut_consumed(ctx, &self.app_config.shortcuts.toggle_razor) {
            self.razor_active = !self.razor_active;
        }

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
                    if ui.button("Export Project").clicked() {
                        self.show_export_dialog = true;
                    }
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
                if ui.button("Undo").on_hover_text("Undo").clicked() {
                    self.undo_stack.undo(&mut self.project, &mut self.timeline);
                }
                if ui.button("Redo").on_hover_text("Redo").clicked() {
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
                self.draw_recovery_banner(ui);
                if ui
                    .button("Fullscreen")
                    .on_hover_text("Fullscreen")
                    .clicked()
                {
                    request_toggle_fullscreen = true;
                }
                ui.separator();
                if ui.button("Export").on_hover_text("Export Video").clicked() {
                    self.show_export_dialog = true;
                }
                if let Some(notification) = &self.export_notification {
                    ui.separator();
                    ui.label(notification);
                    if ui.button("Clear").clicked() {
                        self.export_notification = None;
                    }
                }
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
                state_json["test_first_clip_scale_x"] =
                    clip.scale_x.get_value_at(self.playhead_ms).into();
                state_json["test_first_clip_brightness"] =
                    clip.brightness.get_value_at(self.playhead_ms).into();
                state_json["test_first_clip_speed"] =
                    clip.speed.get_value_at(self.playhead_ms).into();
                state_json["test_first_clip_opacity"] =
                    clip.opacity.get_value_at(self.playhead_ms).into();
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
                    ui.separator();
                    ui.heading("Keyboard Shortcuts");
                    changed |= ui
                        .horizontal(|ui| {
                            ui.label("Play/Pause shortcut");
                            ui.text_edit_singleline(&mut self.app_config.shortcuts.play_pause)
                                .changed()
                        })
                        .inner;
                    changed |= ui
                        .horizontal(|ui| {
                            ui.label("Undo shortcut");
                            ui.text_edit_singleline(&mut self.app_config.shortcuts.undo)
                                .changed()
                        })
                        .inner;
                    changed |= ui
                        .horizontal(|ui| {
                            ui.label("Redo shortcut");
                            ui.text_edit_singleline(&mut self.app_config.shortcuts.redo)
                                .changed()
                        })
                        .inner;
                    changed |= ui
                        .horizontal(|ui| {
                            ui.label("Razor shortcut");
                            ui.text_edit_singleline(&mut self.app_config.shortcuts.toggle_razor)
                                .changed()
                        })
                        .inner;
                    ui.label("Use shortcuts like Space, P, Ctrl+Z, or Ctrl+Shift+R.");
                });
            self.show_preferences_dialog = open;
            if changed {
                self.apply_theme(ctx);
                self.persist_app_config();
            }
        }

        self.process_active_export_seek(ctx);
        self.draw_export_dialog(ctx);

        // BOTTOM PANEL: Timeline
        egui::TopBottomPanel::bottom("timeline_panel")
            .resizable(true)
            .min_height(250.0)
            .show(ctx, |ui| {
                // Timeline Toolbar
                ui.horizontal(|ui| {
                    if ui
                        .button("\u{2795} Track")
                        .on_hover_text("Add Track")
                        .clicked()
                    {
                        if let Err(err) = self.apply_track_mutation(|project| {
                            project.add_track();
                            Ok(())
                        }) {
                            log::error!("Failed to add track: {}", err);
                        }
                    }
                    if ui
                        .button("\u{1F9F2}")
                        .on_hover_text("Snapping Enabled")
                        .clicked()
                    {}
                    ui.toggle_value(&mut self.razor_active, "\u{2702}")
                        .on_hover_text("Razor Tool");
                    if ui
                        .button("\u{1F4CC}")
                        .on_hover_text("Add or Remove Marker at Playhead")
                        .clicked()
                    {
                        self.toggle_marker_at_playhead();
                    }
                    if ui
                        .button("\u{23EE}")
                        .on_hover_text("Previous Marker")
                        .clicked()
                    {
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

                    if ui.rect_contains_pointer(timeline_rect) {
                        let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
                        if ui.input(|i| i.modifiers.ctrl) && scroll_delta != 0.0 {
                            self.zoom_level = (self.zoom_level + scroll_delta * 0.001).clamp(0.0, 1.0);
                        }
                    }

                    let mut current_frame_track_origin_x: Option<f32> = None;

                    // Time ruler interaction
                    let (ruler_rect, ruler_res) = ui.allocate_exact_size(
                        egui::vec2(2000.0, 20.0),
                        egui::Sense::click_and_drag(),
                    );
                    let origin_x = self.last_track_content_origin_x.unwrap_or(ruler_rect.left() + TIMELINE_CONTENT_OFFSET_X);
                    
                    if ruler_res.drag_started() {
                        self.was_playing_before_scrub = self.is_playing;
                        if self.is_playing {
                            self.is_playing = false;
                            if let Some(engine_manager) = &mut self.engine_manager {
                                engine_manager.pause_all();
                            }
                        }
                    }

                    let ruler_context_time_ms = ruler_res.interact_pointer_pos().map(|pos| {
                        Self::timeline_time_from_ruler_position(origin_x, pos, self.zoom_level)
                    });

                    if ruler_res.clicked() || ruler_res.dragged() {
                        if let Some(pos) = ruler_res.interact_pointer_pos() {
                            let mut target_playhead_ms = Self::timeline_time_from_ruler_position(
                                origin_x,
                                pos,
                                self.zoom_level,
                            );

                            if ruler_res.clicked() {
                                let tolerance_px = 6.0;
                                if let Some(marker) = self.project.markers.iter().find(|marker| {
                                    let marker_x = ruler_rect.left()
                                        + (marker.time_ms as f32 * self.zoom_level * 0.1);
                                    (marker_x - pos.x).abs() <= tolerance_px
                                }) {
                                    target_playhead_ms = marker.time_ms;
                                    self.selected_marker_id = Some(marker.id);
                                } else {
                                    self.selected_marker_id = None;
                                }
                            }
                            self.scrub_playhead(target_playhead_ms);
                        }
                    }

                    if ruler_res.drag_released() {
                        if self.was_playing_before_scrub {
                            self.is_playing = true;
                            let (active_clips, _) = self.active_clips_for_playhead();
                            if let Some(engine_manager) = &mut self.engine_manager {
                                let active_ids: Vec<usize> = active_clips.iter().map(|c| c.id).collect();
                                engine_manager.play_clips(&active_ids);
                            }
                            self.was_playing_before_scrub = false;
                        }
                    }

                    ruler_res.context_menu(|ui| {
                        if let Some(time_ms) = ruler_context_time_ms {
                            if ui.button("Add Marker").clicked() {
                                let marker = self.project.add_marker(time_ms);
                                self.selected_marker_id = Some(marker.id);
                                self.playhead_ms = marker.time_ms;
                                self.seek_playhead();
                                ui.close_menu();
                            }
                        } else {
                            ui.label("Right-click on the ruler to add a marker.");
                        }
                    });

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
                    let mut remove_ruler_marker_id: Option<usize> = None;
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

                        let marker_handle_rect = egui::Rect::from_min_max(
                            egui::pos2(marker_x - 10.0, ruler_rect.top()),
                            egui::pos2(marker_x + 10.0, ruler_rect.bottom()),
                        );

                        let marker_res = ui.interact(
                            marker_handle_rect,
                            egui::Id::new(("marker", marker.id)),
                            egui::Sense::click_and_drag(),
                        );

                        if marker_res.dragged() {
                            if let Some(pos) = marker_res.interact_pointer_pos() {
                                let local_x = (pos.x
                                    - (ruler_rect.left() + TIMELINE_CONTENT_OFFSET_X))
                                    .max(0.0);
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

                        marker_res.context_menu(|ui| {
                            if ui.button("Select Marker").clicked() {
                                self.selected_marker_id = Some(marker.id);
                                ui.close_menu();
                            }
                            if ui.button("Delete Marker").clicked() {
                                self.selected_marker_id = Some(marker.id);
                                remove_ruler_marker_id = Some(marker.id);
                                ui.close_menu();
                            }
                            ui.separator();
                            ui.label("Label");
                            ui.text_edit_singleline(&mut marker.label);
                            ui.label("Color");
                            let mut marker_color = Self::marker_color(marker);
                            if ui.color_edit_button_srgba(&mut marker_color).changed() {
                                Self::set_marker_color(marker, marker_color);
                            }
                        });

                        let flag_rect = egui::Rect::from_min_size(
                            egui::pos2(marker_x - 7.0, ruler_rect.top() + 2.0),
                            egui::vec2(14.0, 14.0),
                        );
                        ui.painter().rect_filled(flag_rect, 3.0, color);
                        ui.painter().line_segment(
                            [
                                egui::pos2(marker_x, flag_rect.bottom()),
                                egui::pos2(marker_x, ruler_rect.bottom()),
                            ],
                            egui::Stroke::new(stroke_width, color),
                        );
                        if self.selected_marker_id == Some(marker.id) || marker_res.hovered() {
                            let bubble_width =
                                (marker.label.chars().count() as f32 * 6.5 + 18.0).min(160.0);
                            let bubble_x = if marker_x + bubble_width + 14.0 < ruler_rect.right() {
                                marker_x + 12.0
                            } else {
                                (marker_x - bubble_width - 12.0).max(ruler_rect.left() + 8.0)
                            };
                            let bubble_rect = egui::Rect::from_min_size(
                                egui::pos2(bubble_x, ruler_rect.top() + 1.0),
                                egui::vec2(bubble_width, 18.0),
                            );
                            ui.painter().rect_filled(
                                bubble_rect,
                                4.0,
                                egui::Color32::from_black_alpha(160),
                            );
                            ui.painter().rect_stroke(
                                bubble_rect,
                                4.0,
                                egui::Stroke::new(1.0, color),
                            );
                            ui.painter().text(
                                bubble_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                &marker.label,
                                egui::FontId::proportional(10.0),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                    if should_seek {
                        self.seek_playhead();
                    }
                    if let Some(marker_id) = remove_ruler_marker_id {
                        self.project.markers.retain(|marker| marker.id != marker_id);
                        if self.selected_marker_id == Some(marker_id) {
                            self.selected_marker_id = None;
                        }
                    }
                    if let Some(id) = dragging_marker_id {
                        self.selected_marker_id = Some(id);
                    }
                    ui.add_space(4.0);

                    // Playhead drawing moved to after the track loop so it can use the exact track content origin.

                    let mut razor_cmd = None;
                    let mut add_clip_cmd = None;
                    let mut separate_audio_cmd = None;
                    let mut remove_clip_id = None;
                    let mut pending_track_reorder = None;
                    let mut pending_track_remove_id = None;
                    let mut pending_ungroup_clip_id = None;
                    let mut pending_track_rename: Option<(usize, String)> = None;
                    let mut pending_title_clip_request: Option<(usize, usize, i64)> = None;
                    let track_count = self.project.tracks.len();
                    let track_ids: Vec<usize> =
                        self.project.tracks.iter().map(|track| track.id).collect();

                    // Tracks (OpenShot numbers tracks bottom-up by default)
                    for (track_order_index, track) in self.project.tracks.iter_mut().enumerate() {
                        let track_idx = track.id;
                        let split_audio_track_id =
                            Self::split_audio_target_track_id(&track_ids, track_idx)
                                .unwrap_or(track_idx);
                        ui.horizontal(|ui| {
                            // Track Header
                            let header_response = ui
                                .allocate_ui(egui::vec2(TRACK_HEADER_WIDTH, TRACK_HEIGHT + 6.0), |ui| {
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
                                    (header_response.drag_delta().y / (TRACK_HEIGHT + 8.0)).round() as isize;
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
                                egui::vec2(2000.0, TRACK_HEIGHT),
                                egui::Sense::hover(),
                            );
                            if current_frame_track_origin_x.is_none() {
                                current_frame_track_origin_x = Some(rect.left());
                            }

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
                                        let asset_duration_ms = asset_duration_by_id
                                            .get(&asset_id)
                                            .copied()
                                            .unwrap_or(0);
                                        let end_ms = timeline_clip_end_ms(
                                            start_ms,
                                            asset_duration_ms,
                                            frame_duration_ms,
                                        );

                                        // Create add command
                                        let mut clip = Clip::new(
                                            rand::random::<usize>(),
                                            asset_id,
                                            track_idx,
                                            start_ms,
                                            end_ms,
                                            0,
                                            asset_duration_ms as i64,
                                        );
                                        clip.audio_enabled = self
                                            .project
                                            .assets
                                            .iter()
                                            .find(|asset| asset.id == asset_id)
                                            .map(Self::media_asset_has_audio)
                                            .unwrap_or(false);
                                        clip.video_enabled = true;
                                        add_clip_cmd = Some(AddClipCommand { clip });
                                        // Clear dragging state
                                        ctx.data_mut(|d| {
                                            d.remove_temp::<usize>(egui::Id::new(
                                                "dragging_asset_id",
                                            ))
                                        });
                                    }
                                } else if let Some(title_id) = dragging_title_id {
                                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                                        let local_x = pos.x - rect.left();
                                        let start_ms = (local_x / (self.zoom_level * 0.1)) as i64;
                                        pending_title_clip_request = Some((title_id, track_idx, start_ms));
                                        ctx.data_mut(|d| {
                                            d.remove_temp::<usize>(egui::Id::new(
                                                "dragging_title_id",
                                            ))
                                        });
                                    }
                                }
                            } else if ui.rect_contains_pointer(rect)
                                && ctx.input(|i| i.pointer.any_down())
                            {
                                // Ghost clip preview
                                let dragging_id: Option<usize> =
                                    ctx.data(|d| d.get_temp(egui::Id::new("dragging_asset_id")));
                                let dragging_title_id: Option<usize> =
                                    ctx.data(|d| d.get_temp(egui::Id::new("dragging_title_id")));

                                if let Some(asset_id) = dragging_id.or(dragging_title_id) {
                                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                                        let local_x = pos.x - rect.left();
                                        let start_ms = (local_x / (self.zoom_level * 0.1)) as i64;
                                        let asset_duration_ms = if dragging_id.is_some() {
                                            asset_duration_by_id.get(&asset_id).copied().unwrap_or(0)
                                        } else {
                                            10_000 // Default title duration
                                        };
                                        let pixels_per_ms = (self.zoom_level * 0.1).max(0.001);
                                        let ghost_width = asset_duration_ms as f32 * pixels_per_ms;
                                        let ghost_rect = egui::Rect::from_min_size(
                                            egui::pos2(rect.left() + (start_ms as f32 * pixels_per_ms), rect.top() + 5.0),
                                            egui::vec2(ghost_width, TRACK_HEIGHT - 10.0),
                                        );
                                        ui.painter().rect_filled(
                                            ghost_rect,
                                            4.0,
                                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 40),
                                        );
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
                                        let is_active_drag = self.drag_clip_id == Some(clip.id);
                                        let preview_track_id =
                                            if is_active_drag && self.trim_side.is_none() {
                                                self.drag_preview_track_index
                                                    .and_then(|index| track_ids.get(index).copied())
                                                    .unwrap_or(clip.track_id)
                                            } else {
                                                clip.track_id
                                            };
                                        if is_active_drag
                                            && self.trim_side.is_none()
                                            && preview_track_id != track_idx
                                        {
                                            continue;
                                        }
                                        let pixels_per_ms = (self.zoom_level * 0.1).max(0.001);
                                        let is_active_drag = self.drag_clip_id == Some(clip.id);
                                        let (display_start, display_end) = if is_active_drag {
                                            self.drag_preview_times
                                                .unwrap_or((clip.start, clip.end))
                                        } else {
                                            (clip.start, clip.end)
                                        };
                                        let x_start =
                                            rect.left() + (display_start as f32 * pixels_per_ms);
                                        let x_end =
                                            rect.left() + (display_end as f32 * pixels_per_ms);

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
                                            let (asset_name, audio_levels, asset_uri) = self
                                                .project
                                                .assets
                                                .iter()
                                                .find(|asset| asset.id == clip.asset_id)
                                                .map(|asset| {
                                                    (
                                                        asset.name.clone(),
                                                        asset.audio_levels.clone(),
                                                        asset.uri.clone(),
                                                    )
                                                })
                                                .unwrap_or_else(|| {
                                                    (
                                                        "Unknown Media".to_string(),
                                                        Vec::new(),
                                                        "".to_string(),
                                                    )
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
                                                let pointer_start = ctx
                                                    .input(|input| input.pointer.interact_pos())
                                                    .or_else(|| {
                                                        left_res
                                                            .interact_pointer_pos()
                                                            .or_else(|| {
                                                                right_res.interact_pointer_pos()
                                                            })
                                                            .or_else(|| {
                                                                body_res.interact_pointer_pos()
                                                            })
                                                    });
                                                self.drag_start_times =
                                                    Some((clip.start, clip.end));
                                                self.drag_pointer_start_x =
                                                    pointer_start.map(|pos| pos.x);
                                                self.drag_pointer_start_y =
                                                    pointer_start.map(|pos| pos.y);
                                                self.drag_clip_id = Some(clip.id);
                                                if let Some(group_id) = clip.group_id {
                                                    self.dragged_group_clip_ids = self
                                                        .timeline
                                                        .clips
                                                        .iter()
                                                        .filter(|c| {
                                                            c.group_id == Some(group_id)
                                                                && c.id != clip.id
                                                        })
                                                        .map(|c| c.id)
                                                        .collect();
                                                } else {
                                                    self.dragged_group_clip_ids.clear();
                                                }
                                                self.drag_start_track_index =
                                                    Some(track_order_index);
                                                self.drag_preview_track_index =
                                                    Some(track_order_index);
                                                self.drag_preview_times =
                                                    Some((clip.start, clip.end));
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
                                                        let split_at = Self::clip_split_time_from_pointer(
                                                            clip.start,
                                                            rect.left(),
                                                            pos.x,
                                                            pixels_per_ms,
                                                        );
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
                                                    audio_clip.track_id = split_audio_track_id;
                                                    audio_clip.video_enabled = false;
                                                    audio_clip.audio_enabled = true;

                                                    let group_id = clip.group_id.unwrap_or_else(rand::random::<usize>);
                                                    audio_clip.group_id = Some(group_id);

                                                    separate_audio_cmd =
                                                        Some(SeparateAudioCommand {
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
                                                if clip.group_id.is_some() {
                                                    if ui.button("Ungroup").clicked() {
                                                        pending_ungroup_clip_id = Some(clip.id);
                                                        ui.close_menu();
                                                    }
                                                }
                                            });

                                            let is_selected =
                                                self.selected_clip_id == Some(clip.id);
                                            let is_dragging = is_active_drag
                                                || left_res.dragged()
                                                || right_res.dragged()
                                                || body_res.dragged();
                                            let kind = Self::asset_library_kind(&asset_uri);
                                            let base_color = Self::asset_drag_preview_accent(kind);

                                            let color = if is_selected {
                                                egui::Color32::from_rgb(255, 170, 0)
                                            } else if is_dragging {
                                                egui::Color32::from_rgb(100, 160, 255)
                                            } else {
                                                base_color
                                            };

                                            if let Some(frame) = self.texture_pool.as_ref().and_then(|pool| pool.frame_for_clip(clip.id)) {
                                                if let Some(texture_id) = frame.egui_id {
                                                    let thumb_height = paint_rect.height();
                                                    let thumb_width = (thumb_height * 1.77).max(10.0);
                                                    
                                                    let mut x = paint_rect.left();
                                                    while x < paint_rect.right() {
                                                        let draw_width = thumb_width.min(paint_rect.right() - x);
                                                        let uv_right = draw_width / thumb_width;
                                                        
                                                        ui.painter().image(
                                                            texture_id,
                                                            egui::Rect::from_min_size(egui::pos2(x, paint_rect.top()), egui::vec2(draw_width, thumb_height)),
                                                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(uv_right, 1.0)),
                                                            egui::Color32::from_white_alpha(180),
                                                        );
                                                        x += thumb_width + 4.0;
                                                    }
                                                }
                                            }

                                            let has_thumbnail = self.texture_pool.as_ref()
                                                .and_then(|pool| pool.frame_for_clip(clip.id))
                                                .is_some();
                                            let fill_alpha = if has_thumbnail {
                                                120
                                            } else {
                                                255
                                            };

                                            Self::draw_clip_waveform(
                                                ui,
                                                paint_rect,
                                                &audio_levels,
                                                clip.in_point,
                                                clip.out_point,
                                                self.project
                                                    .assets
                                                    .iter()
                                                    .find(|a| a.id == clip.asset_id)
                                                    .map(|a| a.duration_ms)
                                                    .unwrap_or(0),
                                            );

                                            ui.painter().rect_filled(
                                                paint_rect,
                                                4.0,
                                                egui::Color32::from_rgba_unmultiplied(
                                                    color.r(),
                                                    color.g(),
                                                    color.b(),
                                                    fill_alpha,
                                                ),
                                            );

                                            ui.painter().rect_stroke(
                                                paint_rect,
                                                4.0,
                                                egui::Stroke::new(2.0, egui::Color32::from_rgb(
                                                    clip.color_tag[0],
                                                    clip.color_tag[1],
                                                    clip.color_tag[2],
                                                )),
                                            );
                                            ui.painter().rect_filled(
                                                egui::Rect::from_min_max(
                                                    paint_rect.min,
                                                    egui::pos2(
                                                        paint_rect.right(),
                                                        (paint_rect.top() + 18.0)
                                                            .min(paint_rect.bottom()),
                                                    ),
                                                ),
                                                4.0,
                                                egui::Color32::from_black_alpha(110),
                                            );
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
                                                    paint_rect.top() + 10.0,
                                                ),
                                                egui::Align2::CENTER_CENTER,
                                                clip_label.clone(),
                                                egui::FontId::proportional(12.0),
                                                egui::Color32::WHITE,
                                            );
                                        }
                                    }
                                }

                                if let Some(active_drag_clip_id) = self.drag_clip_id {
                                    if self.trim_side.is_none() {
                                        if let (
                                            Some(start_track_index),
                                            Some(preview_track_index),
                                        ) = (
                                            self.drag_start_track_index,
                                            self.drag_preview_track_index,
                                        ) {
                                            if preview_track_index != start_track_index
                                                && preview_track_index == track_order_index
                                            {
                                                if let Some(clip_index) = self
                                                    .timeline
                                                    .clips
                                                    .iter()
                                                    .position(|c| c.id == active_drag_clip_id)
                                                {
                                                    let clip =
                                                        self.timeline.clips[clip_index].clone();
                                                    let pixels_per_ms =
                                                        (self.zoom_level * 0.1).max(0.001);
                                                    let (display_start, display_end) = self
                                                        .drag_preview_times
                                                        .unwrap_or((clip.start, clip.end));
                                                    let x_start = rect.left()
                                                        + (display_start as f32 * pixels_per_ms);
                                                    let x_end = rect.left()
                                                        + (display_end as f32 * pixels_per_ms);
                                                    let clip_rect = egui::Rect::from_min_max(
                                                        egui::pos2(x_start, rect.top() + 5.0),
                                                        egui::pos2(x_end, rect.bottom() - 5.0),
                                                    );

                                                    #[cfg(target_arch = "wasm32")]
                                                    {
                                                        if self.test_first_clip_rect.is_none() {
                                                            self.test_first_clip_rect =
                                                                Some(clip_rect);
                                                        }
                                                    }

                                                    if clip_rect.min.x < rect.right()
                                                        && clip_rect.max.x > rect.left()
                                                    {
                                                        let paint_rect = clip_rect.intersect(rect);
                                                        let (asset_name, _audio_levels) = self
                                                            .project
                                                            .assets
                                                            .iter()
                                                            .find(|asset| asset.id == clip.asset_id)
                                                            .map(|asset| {
                                                                (
                                                                    asset.name.clone(),
                                                                    asset.audio_levels.clone(),
                                                                )
                                                            })
                                                            .unwrap_or_else(|| {
                                                                (
                                                                    "Unknown Media".to_string(),
                                                                    Vec::new(),
                                                                )
                                                            });
                                                        let clip_label = timeline_clip_label(
                                                            &asset_name,
                                                            clip.id,
                                                            ((paint_rect.width() / 7.0).floor()
                                                                as usize)
                                                                .max(8),
                                                        );
                                                        ui.painter().rect_filled(
                                                            paint_rect,
                                                            4.0,
                                                            egui::Color32::from_rgb(100, 160, 255),
                                                        );
                                                        ui.painter().rect_stroke(
                                                            paint_rect,
                                                            4.0,
                                                            egui::Stroke::new(2.0, egui::Color32::from_rgb(
                                                                clip.color_tag[0],
                                                                clip.color_tag[1],
                                                                clip.color_tag[2],
                                                            )),
                                                        );
                                                        ui.painter().text(
                                                            egui::pos2(
                                                                paint_rect.center().x,
                                                                paint_rect.top() + 14.0,
                                                            ),
                                                            egui::Align2::CENTER_CENTER,
                                                            clip_label,
                                                            egui::FontId::proportional(12.0),
                                                            egui::Color32::WHITE,
                                                        );
                                                    }
                                                }
                                            }
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
                                Some(pointer_start_y),
                                Some(current_pointer_pos),
                            ) = (
                                self.drag_start_times,
                                self.drag_pointer_start_x,
                                self.drag_pointer_start_y,
                                ctx.input(|input| input.pointer.latest_pos()),
                            ) {
                                let pixels_per_ms = (self.zoom_level * 0.1).max(0.001);
                                let snap_radius_ms = ((12.0 / pixels_per_ms).round() as i64).max(1);
                                let delta_x = current_pointer_pos.x - pointer_start_x;
                                let delta_ms = (delta_x / pixels_per_ms) as i64;
                                let (new_start, new_end) = if let Some(side) = self.trim_side {
                                    let (preview_start, preview_end) = self.timeline.preview_trim(
                                        old_start,
                                        old_end,
                                        side == TrimSide::Left,
                                        delta_ms,
                                        snap_radius_ms,
                                        &[old_start, old_end],
                                    );
                                    let asset_duration_ms = self
                                        .timeline
                                        .clips
                                        .iter()
                                        .find(|clip| clip.id == active_drag_clip_id)
                                        .and_then(|clip| asset_duration_by_id.get(&clip.asset_id))
                                        .copied()
                                        .unwrap_or(0);
                                    clamp_trim_to_asset_bounds(
                                        preview_start,
                                        preview_end,
                                        side == TrimSide::Left,
                                        asset_duration_ms,
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
                                if self.trim_side.is_none() {
                                    if let Some(start_track_index) = self.drag_start_track_index {
                                        let track_count = self.project.tracks.len();
                                        if track_count > 0 {
                                            let delta_y = current_pointer_pos.y - pointer_start_y;
                                            let row_offset = (delta_y / (TRACK_HEIGHT + 8.0)).round() as isize;
                                            let target_track_index = (start_track_index as isize
                                                + row_offset)
                                                .clamp(0, track_count.saturating_sub(1) as isize)
                                                as usize;
                                            self.drag_preview_track_index =
                                                Some(target_track_index);
                                        }
                                    }
                                } else {
                                    self.drag_preview_track_index = self.drag_start_track_index;
                                }
                            }
                        } else {
                            if let (Some((old_start, _old_end)), Some((new_start, new_end))) =
                                (self.drag_start_times, self.drag_preview_times)
                            {
                                let delta_ms = new_start - old_start;
                                let track_delta = self.drag_preview_track_index.map(|idx| idx as isize).unwrap_or(0) - self.drag_start_track_index.map(|idx| idx as isize).unwrap_or(0);

                                let mut affected_ids = vec![active_drag_clip_id];
                                if self.trim_side.is_none() {
                                    affected_ids.extend(self.dragged_group_clip_ids.iter().copied());
                                }

                                let mut commands: Vec<Box<dyn clipstrike_core::commands::Command>> = Vec::new();

                                for cid in affected_ids {
                                    if let Some(clip) = self.timeline.clips.iter().find(|c| c.id == cid) {
                                        let c_old_start = clip.start;
                                        let c_old_end = clip.end;

                                        if self.trim_side.is_some() && cid == active_drag_clip_id {
                                            commands.push(Box::new(TrimClipCommand {
                                                clip_id: cid,
                                                old_start: c_old_start,
                                                old_end: c_old_end,
                                                new_start,
                                                new_end,
                                            }));
                                        } else {
                                            let c_new_start = c_old_start + delta_ms;
                                            let c_new_end = c_old_end + delta_ms;

                                            let old_track_id = clip.track_id;
                                            let current_track_idx = track_ids.iter().position(|&id| id == old_track_id).unwrap_or(0);
                                            let new_track_idx = (current_track_idx as isize + track_delta).clamp(0, track_ids.len() as isize - 1) as usize;
                                            let new_track_id = track_ids[new_track_idx];

                                            commands.push(Box::new(MoveClipCommand {
                                                clip_id: cid,
                                                old_start: c_old_start,
                                                old_end: c_old_end,
                                                new_start: c_new_start,
                                                new_end: c_new_end,
                                                old_track_id,
                                                new_track_id,
                                            }));
                                        }
                                    }
                                }

                                if !commands.is_empty() {
                                    if commands.len() == 1 {
                                        self.undo_stack.push(commands.pop().unwrap(), &mut self.project, &mut self.timeline);
                                    } else {
                                        self.undo_stack.push(Box::new(CompoundCommand { commands }), &mut self.project, &mut self.timeline);
                                    }
                                }
                            }
                            self.drag_start_times = None;
                            self.drag_pointer_start_x = None;
                            self.drag_pointer_start_y = None;
                            self.drag_clip_id = None;
                            self.dragged_group_clip_ids.clear();
                            self.drag_start_track_index = None;
                            self.drag_preview_times = None;
                            self.drag_preview_track_index = None;
                            self.trim_side = None;
                        }
                    }

                    self.last_track_content_origin_x = current_frame_track_origin_x;

                    // Draw Playhead line across all tracks using the exact track content origin
                    if let Some(origin_x) = current_frame_track_origin_x {
                        let playhead_x = origin_x + (self.playhead_ms as f32 * self.zoom_level * 0.1);
                        if playhead_x >= timeline_rect.left() && playhead_x <= timeline_rect.right() {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(playhead_x, timeline_rect.top()),
                                    egui::pos2(playhead_x, timeline_rect.bottom()),
                                ],
                                egui::Stroke::new(2.0, egui::Color32::RED),
                            );
                        }
                    } else {
                        // Fallback if no tracks exist
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
                    }

                    if let Some((id, split_at)) = razor_cmd {
                        self.undo_stack.push(
                            Box::new(clipstrike_core::commands::RazorClipCommand {
                                clip_id: id,
                                split_at,
                                original_clip: None,
                                new_clip_id: None,
                            }),
                            &mut self.project,
                            &mut self.timeline,
                        );
                    }

                    if let Some((title_id, track_id, start_ms)) = pending_title_clip_request {
                        match self.create_title_clip(title_id, track_id, start_ms, 5_000) {
                            Ok(clip) => add_clip_cmd = Some(AddClipCommand { clip }),
                            Err(err) => log::error!(
                                "Failed to drag title {} onto timeline: {}",
                                title_id,
                                err
                            ),
                        }
                    }

                    if let Some(cmd) = add_clip_cmd {
                        self.undo_stack
                            .push(Box::new(cmd), &mut self.project, &mut self.timeline);
                    }

                    if let Some(cmd) = separate_audio_cmd {
                        self.undo_stack
                            .push(Box::new(cmd), &mut self.project, &mut self.timeline);
                    }

                    if let Some(clip_id) = pending_ungroup_clip_id {
                        let _ = self.apply_track_mutation(|project| {
                            if let Some(pos) = project.clips.iter().position(|c| c.id == clip_id) {
                                project.clips[pos].group_id = None;
                            }
                            Ok(())
                        });
                        self.timeline = self.project.to_timeline();
                    }

                    if let Some(id) = remove_clip_id {
                        if let Some(pos) = self.timeline.clips.iter().position(|c| c.id == id)
                        {
                            let clip = self.timeline.clips[pos].clone();
                            self.undo_stack.push(
                                Box::new(RemoveClipCommand { clip }),
                                &mut self.project,
                                &mut self.timeline,
                            );
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
                                    let asset_ids: Vec<usize> = self
                                        .project
                                        .assets
                                        .iter()
                                        .filter(|asset| {
                                            if !self.asset_filter_text.is_empty()
                                                && !asset.name.to_lowercase().contains(
                                                    &self.asset_filter_text.to_lowercase(),
                                                )
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
                                                1 => ["mp4", "mkv", "mov", "avi"]
                                                    .contains(&ext.as_str()),
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
                                            let asset = self
                                                .project
                                                .assets
                                                .iter()
                                                .find(|a| a.id == id)
                                                .unwrap();
                                            (
                                                asset.id,
                                                asset.name.clone(),
                                                asset.uri.clone(),
                                                asset.duration_ms,
                                            )
                                        };
                                        let (asset_id, asset_name, asset_uri, asset_duration_ms) =
                                            asset_data;

                                        let item_id = egui::Id::new(("asset", asset_id));

                                        let mut add_to_timeline = false;
                                        ui.horizontal(|ui| {
                                            ui.label(Self::asset_library_kind(&asset_uri));
                                            let label_res = ui.selectable_label(false, &asset_name);

                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                if self.test_first_asset_rect.is_none() {
                                                    self.test_first_asset_rect =
                                                        Some(label_res.rect);
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
                                            if let Err(err) = self
                                                .add_asset_to_timeline(asset_id, asset_duration_ms)
                                            {
                                                log::error!(
                                                    "Failed to add asset {} to timeline: {}",
                                                    asset_id,
                                                    err
                                                );
                                            }
                                        }
                                    }
                                }
                            });

                            #[cfg(debug_assertions)]
                            {
                                ui.separator();
                                if ui.button("Apply Motion Track (Test)").clicked() {
                                    self.apply_motion_track_test();
                                }
                                if ui.button("Apply Detection Labels (Test)").clicked() {
                                    self.apply_detection_labels_test();
                                }
                                let snapshot = self.test_snapshot();
                                ui.label(format!(
                                    "__clipstrike_test_clip_count__: {}",
                                    snapshot["clips_count"].as_u64().unwrap_or(0)
                                ));
                                ui.label(format!(
                                    "__clipstrike_test_motion_track_count__: {}",
                                    snapshot["tracking_data_count"].as_u64().unwrap_or(0)
                                ));
                                ui.label(format!(
                                    "__clipstrike_test_detection_label_count__: {}",
                                    snapshot["detection_count"].as_u64().unwrap_or(0)
                                ));
                                ui.label(format!(
                                    "__clipstrike_test_playback_complete__: {}",
                                    snapshot["playback_complete"].as_bool().unwrap_or(false)
                                ));
                            }
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
                                    let title_ids: Vec<usize> =
                                        self.project.titles.iter().map(|t| t.id).collect();
                                    for id in title_ids {
                                        let title_data = {
                                            let title = self
                                                .project
                                                .titles
                                                .iter()
                                                .find(|t| t.id == id)
                                                .unwrap();
                                            (title.id, title.name.clone())
                                        };
                                        let (title_id, title_name) = title_data;

                                        let mut add_to_timeline = false;
                                        ui.horizontal(|ui| {
                                            ui.label("\u{1F3F7}");
                                            let title_label_res = ui.label(&title_name);

                                            #[cfg(not(target_arch = "wasm32"))]
                                            title_label_res.context_menu(|ui| {
                                                if ui.button("Edit in Inkscape").clicked() {
                                                    if let Err(err) = self.edit_title_in_inkscape(title_id) {
                                                        log::error!(
                                                            "Failed to open title {} in Inkscape: {}",
                                                            title_id,
                                                            err
                                                        );
                                                    }
                                                    ui.close_menu();
                                                }
                                            });

                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                if self.test_first_title_rect.is_none() {
                                                    self.test_first_title_rect =
                                                        Some(title_label_res.rect);
                                                }
                                            }

                                            // Task 13.2: Drag source for titles
                                            let item_id = egui::Id::new(("title_asset", title_id));
                                            let drag_label_res =
                                                ui.selectable_label(false, "[Drag to Timeline]");

                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                if self.test_first_title_rect.is_none() {
                                                    self.test_first_title_rect =
                                                        Some(drag_label_res.rect);
                                                }
                                            }

                                            if ui
                                                .interact(
                                                    drag_label_res.rect,
                                                    item_id,
                                                    egui::Sense::drag(),
                                                )
                                                .dragged()
                                            {
                                                ctx.data_mut(|d| {
                                                    d.insert_temp(
                                                        egui::Id::new("dragging_title_id"),
                                                        title_id,
                                                    )
                                                });
                                            }

                                            if ui
                                                .button("\u{2795}")
                                                .on_hover_text("Add to Timeline")
                                                .clicked()
                                            {
                                                add_to_timeline = true;
                                            }
                                        });

                                        if add_to_timeline {
                                            if let Err(err) = self.add_title_to_timeline(title_id, 5_000) {
                                                log::error!(
                                                    "Failed to add title {} to timeline: {}",
                                                    title_id,
                                                    err
                                                );
                                            }
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
        let pending_frame_upload_requests = self.collect_pending_frame_upload_requests();
        let queue = self.queue.clone();
        let device = self.device.clone();
        let wgpu_render_state = self.wgpu_render_state.clone();
        if let (Some(texture_pool), Some(queue), Some(device), Some(wgpu_render_state)) = (
            &mut self.texture_pool,
            queue.as_ref(),
            device.as_ref(),
            wgpu_render_state.as_ref(),
        ) {
            for request in pending_frame_upload_requests {
                let clip_id = request.clip_id;
                match texture_pool.apply_frame_upload_request(queue, request) {
                    Ok(_) => {
                        if let Some(frame) = texture_pool.frame_for_clip(clip_id) {
                            if let Some(egui_id) = frame.egui_id {
                                let mut renderer = wgpu_render_state.renderer.write();
                                renderer.update_egui_texture_from_wgpu_texture(
                                    device,
                                    frame.preview_view(),
                                    wgpu::FilterMode::Linear,
                                    egui_id,
                                );
                            }
                        }
                    }
                    Err(err) => {
                        log::warn!(
                            "Failed to apply frame upload request for clip {}: {}",
                            clip_id,
                            err
                        );
                    }
                }
            }
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

                    let fx_sampler = self.fx_sampler(device);

                    if let Some(texture_pool) = &self.texture_pool {
                        for track in self.visible_tracks_in_render_order() {
                            let mut blend_mode = clipstrike_core::timeline::BlendMode::Normal;
                            let mut saw_clip = false;

                            for clip in self.clips_for_track_at_playhead(track.id) {
                                saw_clip = true;
                                blend_mode = clip.blend_mode;

                                if let Some(frame) = texture_pool.frame_for_clip(clip.id) {
                                    let transform = Self::preview_transform_for_clip(
                                        clip,
                                        self.playhead_ms,
                                        texture_pool.width,
                                        texture_pool.height,
                                    );
                                    let fx_params = FxUniforms {
                                        brightness: clip.brightness.get_value_at(self.playhead_ms),
                                        contrast: clip.contrast.get_value_at(self.playhead_ms),
                                        saturation: clip.saturation.get_value_at(self.playhead_ms),
                                        grayscale: clip.grayscale.get_value_at(self.playhead_ms),
                                        blur_radius: clip
                                            .blur_radius
                                            .get_value_at(self.playhead_ms),
                                        alpha: clip.opacity.get_value_at(self.playhead_ms),
                                        transform,
                                        ..Default::default()
                                    };
                                    fx_pipeline.run(
                                        device,
                                        queue,
                                        frame.preview_view(),
                                        fx_output_view,
                                        None,
                                        fx_sampler,
                                        fx_params,
                                    );
                                }
                            }

                            if !saw_clip {
                                continue;
                            }

                            let compose_params = ComposeUniforms {
                                blend_mode: Self::blend_mode_to_u32(blend_mode),
                                opacity: 1.0,
                                watermark_enabled: 0,
                                watermark_opacity: 1.0,
                                watermark_rect: [0.0, 0.0, 0.0, 0.0],
                            };
                            compositor_pipeline.run(
                                device,
                                queue,
                                fx_output_view,
                                accumulator_view,
                                scratch_view,
                                fx_output_view,
                                fx_sampler,
                                compose_params,
                            );

                            // Copy scratch back to accumulator to avoid resource aliasing in next track
                            let mut copy_encoder =
                                device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                    label: Some("Copy Scratch to Accum"),
                                });
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
                        }
                        if ui.button("<<").on_hover_text("Previous Frame").clicked() {
                            self.playhead_ms = (self.playhead_ms - frame_duration).max(0);
                            self.seek_playhead();
                        }
                        let play_icon = if self.is_playing { "Pause" } else { "Play" };
                        if ui.button(play_icon).clicked() {
                            self.toggle_playback();
                        }
                        if ui.button(">>").on_hover_text("Next Frame").clicked() {
                            self.playhead_ms =
                                (self.playhead_ms + frame_duration).min(timeline_end);
                            self.seek_playhead();
                        }
                        ui.toggle_value(&mut self.loop_enabled, "Loop")
                            .on_hover_text("Toggle Loop");
                        if ui.button(">|").on_hover_text("Jump to End").clicked() {
                            self.playhead_ms = timeline_end;
                            self.seek_playhead();
                        }

                        ui.label(format!(
                            "{} / {}",
                            self.format_timecode(self.playhead_ms),
                            self.format_timecode(timeline_end),
                        ));

                        ui.separator();
                        ui.label("\u{1F50A}");
                        if ui
                            .add(
                                egui::Slider::new(&mut self.master_volume, 0.0..=2.0)
                                    .show_value(false),
                            )
                            .changed()
                        {
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

        self.draw_library_drag_preview(ctx);
        self.process_active_export_capture(ctx);
        self.draw_title_editor(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipstrike_core::project::MediaAsset;
    use std::path::PathBuf;

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

    #[test]
    fn cached_value_initializes_once() {
        let slot = OnceCell::new();
        let mut init_calls = 0;

        let first = ClipStrikeApp::cached_value(&slot, || {
            init_calls += 1;
            7_u32
        });
        assert_eq!(*first, 7);

        let second = ClipStrikeApp::cached_value(&slot, || {
            init_calls += 1;
            8_u32
        });
        assert_eq!(*second, 7);
        assert_eq!(init_calls, 1);
    }

    #[test]
    fn split_audio_target_track_id_prefers_an_existing_neighboring_track() {
        let mut project = Project::new("Split Audio", 1920, 1080, 60.0);
        project.tracks = Project::default_tracks(5);
        let app = ClipStrikeApp::test_harness(project);
        let track_ids: Vec<usize> = app.project.tracks.iter().map(|track| track.id).collect();

        assert_eq!(
            ClipStrikeApp::split_audio_target_track_id(&track_ids, 5),
            Some(4)
        );
        assert_eq!(
            ClipStrikeApp::split_audio_target_track_id(&track_ids, 1),
            Some(2)
        );
        assert_eq!(
            ClipStrikeApp::split_audio_target_track_id(&track_ids, 3),
            Some(4)
        );
    }

    #[test]
    fn visible_tracks_and_track_clips_follow_render_order() {
        let mut project = Project::new("Render", 1920, 1080, 60.0);
        project.tracks = Project::default_tracks(5);

        let mut app = ClipStrikeApp::test_harness(project);
        app.project.tracks[1].visible = false;
        app.project.tracks[3].visible = false;
        app.project.clips = vec![
            Clip::new(1, 1, app.project.tracks[0].id, 0, 100, 0, 100),
            Clip::new(2, 1, app.project.tracks[0].id, 50, 150, 0, 100),
            Clip::new(3, 1, app.project.tracks[2].id, 0, 100, 0, 100),
        ];
        app.timeline = app.project.to_timeline();
        app.playhead_ms = 50;

        let visible_track_ids: Vec<usize> = app
            .visible_tracks_in_render_order()
            .map(|track| track.id)
            .collect();
        assert_eq!(visible_track_ids, vec![1, 3, 5]);

        let track_five_clip_ids: Vec<usize> = app
            .clips_for_track_at_playhead(app.project.tracks[0].id)
            .map(|clip| clip.id)
            .collect();
        assert_eq!(track_five_clip_ids, vec![1, 2]);
    }

    #[test]
    fn add_asset_to_timeline_uses_first_visible_track() {
        let mut app = ClipStrikeApp::test_harness(Project::new("Add Asset", 1920, 1080, 60.0));
        for track in &mut app.project.tracks {
            track.visible = false;
        }
        app.project.tracks[1].visible = true;
        app.project.assets.push(MediaAsset {
            id: 42,
            name: "Imported Media".to_string(),
            uri: "file:///tmp/imported.mov".to_string(),
            duration_ms: 4000,
            audio_levels: vec![],
        });

        app.playhead_ms = 250;
        app.add_asset_to_timeline(42, 4000)
            .expect("asset should be added to an existing visible track");

        let clip = app.timeline.clips.first().expect("clip should be inserted");
        assert_eq!(clip.asset_id, 42);
        assert_eq!(clip.track_id, app.project.tracks[1].id);
        assert_eq!(clip.start, 250);
        assert!(clip.end > clip.start);
    }

    #[test]
    fn add_asset_to_timeline_enables_audio_for_audio_assets() {
        let mut app =
            ClipStrikeApp::test_harness(Project::new("Add Audio Asset", 1920, 1080, 60.0));
        app.playhead_ms = 250;
        app.project.assets.push(MediaAsset {
            id: 7,
            name: "Imported Media".to_string(),
            uri: "file:///tmp/imported.mov".to_string(),
            duration_ms: 4000,
            audio_levels: vec![0.25, 0.75],
        });

        app.add_asset_to_timeline(7, 4000)
            .expect("asset should be added to an existing visible track");

        let clip = app.timeline.clips.first().expect("clip should be inserted");
        assert!(clip.audio_enabled, "audio assets should keep audio enabled");
        assert!(clip.video_enabled, "media assets should keep video enabled");
    }

    #[test]
    fn asset_library_kind_classifies_media_extensions() {
        assert_eq!(ClipStrikeApp::asset_library_kind("file:///clip.mp4"), "Video");
        assert_eq!(ClipStrikeApp::asset_library_kind("file:///clip.wav"), "Audio");
        assert_eq!(ClipStrikeApp::asset_library_kind("file:///still.png"), "Image");
        assert_eq!(ClipStrikeApp::asset_library_kind("file:///unknown.bin"), "Media");
    }

    #[test]
    fn add_title_to_timeline_materializes_a_real_asset() {
        let mut app = ClipStrikeApp::test_harness(Project::new("Add Title", 1920, 1080, 60.0));
        let title_id = rand::random::<usize>();
        app.playhead_ms = 250;
        app.project.titles.push(clipstrike_core::project::TitleAsset {
            id: title_id,
            name: "Lower Third".to_string(),
            template: clipstrike_core::project::TitleTemplate::LowerThird,
            text: "ClipStrike".to_string(),
            font_family: "Arial".to_string(),
            font_size: 48.0,
            color: [255, 255, 255, 255],
            x: 0.5,
            y: 0.8,
            svg_content: "<svg><text>ClipStrike</text></svg>".to_string(),
        });

        app.add_title_to_timeline(title_id, 5_000)
            .expect("title should be added to the timeline");

        assert_eq!(app.project.assets.len(), 1);
        let asset = app.project.assets.first().expect("title asset should exist");
        let clip = app.timeline.clips.first().expect("title clip should be inserted");
        assert_ne!(asset.id, 0, "title assets must never use the missing-asset sentinel");
        assert_eq!(clip.asset_id, asset.id);
        assert!(!clip.audio_enabled, "title clips should stay silent");
        assert!(clip.video_enabled, "title clips should render video");
    }

    #[test]
    fn create_title_clip_reuses_existing_materialized_asset() {
        let mut app = ClipStrikeApp::test_harness(Project::new("Reuse Title", 1920, 1080, 60.0));
        let title_id = rand::random::<usize>();
        app.project.titles.push(clipstrike_core::project::TitleAsset {
            id: title_id,
            name: "Bugfix Title".to_string(),
            template: clipstrike_core::project::TitleTemplate::Centered,
            text: "Bugfix".to_string(),
            font_family: "Arial".to_string(),
            font_size: 48.0,
            color: [255, 255, 255, 255],
            x: 0.5,
            y: 0.5,
            svg_content: "<svg><text>Bugfix</text></svg>".to_string(),
        });

        let track_id = app.project.tracks[0].id;
        let first_clip = app
            .create_title_clip(title_id, track_id, 0, 5_000)
            .expect("first title clip should be created");
        let second_clip = app
            .create_title_clip(title_id, track_id, 5_000, 5_000)
            .expect("second title clip should reuse the same materialized asset");

        assert_eq!(app.project.assets.len(), 1);
        assert_eq!(first_clip.asset_id, second_clip.asset_id);
        assert_ne!(first_clip.asset_id, 0);
    }

    #[test]
    fn playback_audio_samples_treats_empty_buffers_as_streaming_sources() {
        let empty = std::sync::Arc::new(Vec::new());
        assert!(ClipStrikeApp::playback_audio_samples(empty).is_none());

        let samples = std::sync::Arc::new(vec![0.25_f32, -0.25_f32]);
        let returned = ClipStrikeApp::playback_audio_samples(samples.clone())
            .expect("non-empty samples should be retained");
        assert_eq!(returned.as_ref(), samples.as_ref());
    }
    #[test]
    fn import_media_path_uses_encoded_file_uris_for_windows_paths() {
        let mut app = ClipStrikeApp::test_harness(Project::new("Import URI", 1920, 1080, 60.0));
        let source = PathBuf::from(r"C:\Clips\0203 (1)-1.mp4");
        let expected =
            url::Url::from_file_path(&source).expect("windows path should convert to a file URI");

        app.import_media_path(source.clone());

        let asset = app
            .project
            .assets
            .first()
            .expect("asset should be imported");
        assert_eq!(asset.uri, expected.to_string());
        assert!(!asset.uri.contains(' '));
    }

    #[test]
    fn pending_frame_upload_requests_are_collected_from_receiver() {
        let (sender, receiver) = tokio::sync::mpsc::channel(4);
        let project = Project::new("Uploads", 1920, 1080, 60.0);
        let mut app = ClipStrikeApp::test_harness_with_frame_upload_rx(project, receiver);

        sender
            .try_send(clipstrike_render::gpu::FrameUploadRequest {
                clip_id: 7,
                timestamp_ms: 99,
                external_texture_id: Some(12),
                native_d3d12_shared_handle: None,
                pixels: vec![1, 2, 3, 4],
            })
            .expect("send upload request");

        let requests = app.collect_pending_frame_upload_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].clip_id, 7);
        assert_eq!(requests[0].timestamp_ms, 99);
        assert_eq!(requests[0].external_texture_id, Some(12));
    }

    #[test]
    fn recovery_checkpoint_write_uses_current_project_path_when_autosave_enabled() {
        let project = Project::new("Recovery", 1920, 1080, 60.0);
        let mut app = ClipStrikeApp::test_harness(project);
        let checkpoint_source = std::env::temp_dir().join(format!(
            "clipstrike-recovery-{}.cstrike",
            rand::random::<u64>()
        ));

        app.app_config.autosave_enabled = true;
        app.current_project_path = Some(checkpoint_source.clone());
        app.timeline.clips.push(Clip::new(77, 1, 1, 0, 250, 0, 250));

        let checkpoint_path = app
            .write_recovery_checkpoint()
            .expect("recovery checkpoint write should succeed")
            .expect("autosave-enabled sessions should emit a checkpoint");

        assert_eq!(
            checkpoint_path,
            AppConfig::recovery_checkpoint_path_for_project(&checkpoint_source)
        );

        let loaded = Project::load(&checkpoint_path).expect("recovery checkpoint should load");
        assert_eq!(loaded.clips.len(), 1);
        assert_eq!(loaded.clips[0].id, 77);

        let _ = std::fs::remove_file(checkpoint_path);
    }

    #[test]
    fn autosave_checkpoint_writes_after_track_mutation() {
        let project = Project::new("Recovery Mutation", 1920, 1080, 60.0);
        let mut app = ClipStrikeApp::test_harness(project);
        let checkpoint_source = std::env::temp_dir().join(format!(
            "clipstrike-recovery-mutation-{}.cstrike",
            rand::random::<u64>()
        ));

        app.app_config.autosave_enabled = true;
        app.current_project_path = Some(checkpoint_source.clone());

        app.apply_track_mutation(|project| {
            project.clips.push(Clip::new(78, 1, 1, 0, 250, 0, 250));
            Ok(())
        })
        .expect("track mutation should succeed");

        let checkpoint_path = AppConfig::recovery_checkpoint_path_for_project(&checkpoint_source);
        assert!(
            checkpoint_path.exists(),
            "recovery checkpoint should be written after mutation"
        );

        let loaded = Project::load(&checkpoint_path).expect("recovery checkpoint should load");
        assert_eq!(loaded.clips.len(), 1);
        assert_eq!(loaded.clips[0].id, 78);

        let _ = std::fs::remove_file(checkpoint_path);
    }

    #[test]
    fn save_rgba_buffer_to_path_keeps_caller_owned_pixels_buffer_available() {
        let mut pixels = vec![1_u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let first_ptr = pixels.as_ptr();
        let path = std::env::temp_dir().join(format!(
            "clipstrike-save-buffer-{}.png",
            rand::random::<u64>()
        ));

        ClipStrikeApp::save_rgba_buffer_to_path(&path, &mut pixels, 2, 2).expect("save rgba frame");
        assert_eq!(pixels.as_ptr(), first_ptr);
        assert!(path.exists(), "expected save_buffer to write a file");

        ClipStrikeApp::save_rgba_buffer_to_path(&path, &mut pixels, 2, 2)
            .expect("save rgba frame again");
        assert_eq!(pixels.as_ptr(), first_ptr);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn title_svg_temp_path_uses_title_id() {
        let path = ClipStrikeApp::title_svg_temp_path(17);

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("title_17.svg")
        );
    }

    #[test]
    fn refresh_title_svg_from_path_updates_title_and_returns_matching_clip_ids() {
        let mut app = ClipStrikeApp::test_harness(Project::new("Title Refresh", 1920, 1080, 30.0));
        let title_id = rand::random::<usize>();
        let temp_path = ClipStrikeApp::title_svg_temp_path(title_id);
        let updated_svg = "<svg><text>Updated</text></svg>";
        std::fs::write(&temp_path, updated_svg).expect("write temp svg");

        let temp_uri = ClipStrikeApp::file_uri_from_path(&temp_path).expect("temp path uri");
        let track_id = app.project.tracks[0].id;
        let clip_id = rand::random::<usize>();

        app.project
            .titles
            .push(clipstrike_core::project::TitleAsset {
                id: title_id,
                name: "Editable Title".to_string(),
                template: clipstrike_core::project::TitleTemplate::Centered,
                text: "Editable".to_string(),
                font_family: "Arial".to_string(),
                font_size: 48.0,
                color: [255, 255, 255, 255],
                x: 0.5,
                y: 0.5,
                svg_content: "<svg><text>Original</text></svg>".to_string(),
            });
        app.project.assets.push(MediaAsset {
            id: 101,
            name: "title_17.svg".to_string(),
            uri: temp_uri,
            duration_ms: 5000,
            audio_levels: Vec::new(),
        });
        app.project
            .clips
            .push(Clip::new(clip_id, 101, track_id, 0, 5000, 0, 5000));
        app.timeline = app.project.to_timeline();

        let affected_clips = app
            .refresh_title_svg_from_path(title_id, &temp_path)
            .expect("refresh title svg should succeed");

        assert_eq!(affected_clips, vec![clip_id]);
        assert_eq!(
            app.project
                .titles
                .iter()
                .find(|title| title.id == title_id)
                .map(|title| title.svg_content.as_str()),
            Some(updated_svg)
        );

        let _ = std::fs::remove_file(temp_path);
    }

    #[test]
    fn preview_texture_source_kind_recognizes_external_metadata() {
        assert_eq!(
            clipstrike_render::gpu::frame_source_kind(Some(12), false),
            FrameSourceKind::ExternalTexture(12)
        );
        assert_eq!(
            clipstrike_render::gpu::frame_source_kind(None, false),
            FrameSourceKind::NativeTexture
        );
        assert_eq!(
            clipstrike_render::gpu::frame_source_kind(None, true),
            FrameSourceKind::ImportedNativeTexture
        );
    }

    #[test]
    fn preview_bypasses_fx_for_neutral_external_frames() {
        let clip = make_test_clip();

        assert!(ClipStrikeApp::preview_texture_should_bypass_fx(
            &clip,
            0,
            FrameSourceKind::ExternalTexture(12)
        ));
        assert!(!ClipStrikeApp::preview_texture_should_bypass_fx(
            &clip,
            0,
            FrameSourceKind::NativeTexture
        ));
        assert!(ClipStrikeApp::preview_texture_should_bypass_fx(
            &clip,
            0,
            FrameSourceKind::ImportedNativeTexture
        ));

        let mut fx_clip = make_test_clip();
        fx_clip
            .blur_radius
            .insert_keyframe(0, 1.0, InterpolationMode::Linear);
        assert!(!ClipStrikeApp::preview_texture_should_bypass_fx(
            &fx_clip,
            0,
            FrameSourceKind::ExternalTexture(12)
        ));
    }

    #[test]
    fn preview_transform_includes_position_and_stabilization_offset() {
        let mut clip = make_test_clip();
        clip.pos_x
            .insert_keyframe(0, 10.0, InterpolationMode::Linear);
        clip.pos_y
            .insert_keyframe(0, 20.0, InterpolationMode::Linear);
        clip.scale_x
            .insert_keyframe(0, 2.0, InterpolationMode::Linear);
        clip.scale_y
            .insert_keyframe(0, 1.5, InterpolationMode::Linear);
        clip.stabilization_enabled = true;
        clip.stabilization_strength
            .insert_keyframe(0, 1.0, InterpolationMode::Linear);
        clip.tracking_data = vec![MotionTrack {
            id: 7,
            name: "track".to_string(),
            points: vec![
                TrackPoint {
                    time_ms: 0,
                    x: 0.25,
                    y: 0.25,
                    width: 0.1,
                    height: 0.1,
                },
                TrackPoint {
                    time_ms: 1000,
                    x: 0.20,
                    y: 0.30,
                    width: 0.1,
                    height: 0.1,
                },
            ],
        }];

        let transform = ClipStrikeApp::preview_transform_for_clip(&clip, 1000, 1920, 1080);

        assert!((transform[0] - 2.0).abs() < f32::EPSILON);
        assert!((transform[5] - 1.5).abs() < f32::EPSILON);
        assert!((transform[12] - 106.0).abs() < 0.01);
        assert!((transform[13] + 34.0).abs() < 0.01);
    }

    #[test]
    fn toggle_playback_flips_is_playing_state() {
        let project = Project::new("Transport Harness", 1920, 1080, 30.0);
        let mut app = ClipStrikeApp::test_harness(project);

        assert!(!app.is_playing);
        app.toggle_playback();
        assert!(app.is_playing);
        app.toggle_playback();
        assert!(!app.is_playing);
    }

    #[test]
    fn step_playback_clamps_to_timeline_end_and_stops() {
        let mut project = Project::new("Playback End", 1920, 1080, 30.0);
        project.clips = vec![Clip::new(1, 1, 1, 0, 5_000, 0, 5_000)];

        let mut app = ClipStrikeApp::test_harness(project);
        app.timeline = app.project.to_timeline();
        app.playhead_ms = 4_950;
        app.is_playing = true;

        app.step_playback(100);

        assert_eq!(app.playhead_ms, 5_000);
        assert!(!app.is_playing);
    }

    #[test]
    fn handle_clip_eos_stops_when_clip_ends_the_timeline() {
        let mut project = Project::new("Playback EOS Stop", 1920, 1080, 30.0);
        project.clips = vec![Clip::new(1, 1, 1, 0, 5_000, 0, 5_000)];

        let mut app = ClipStrikeApp::test_harness(project);
        app.timeline = app.project.to_timeline();
        app.playhead_ms = 4_980;
        app.is_playing = true;

        app.handle_clip_eos(1);

        assert_eq!(app.playhead_ms, 5_000);
        assert!(!app.is_playing);
    }

    #[test]
    fn handle_clip_eos_keeps_playing_when_next_clip_starts_at_boundary() {
        let mut project = Project::new("Playback EOS Continue", 1920, 1080, 30.0);
        project.clips = vec![
            Clip::new(1, 1, 1, 0, 5_000, 0, 5_000),
            Clip::new(2, 2, 1, 5_000, 8_000, 0, 3_000),
        ];

        let mut app = ClipStrikeApp::test_harness(project);
        app.timeline = app.project.to_timeline();
        app.playhead_ms = 4_980;
        app.is_playing = true;

        app.handle_clip_eos(1);

        assert_eq!(app.playhead_ms, 5_000);
        assert!(app.is_playing);
    }

    #[test]
    fn scrub_playhead_clamps_target_to_timeline_end() {
        let mut project = Project::new("Scrub Clamp", 1920, 1080, 30.0);
        project.clips = vec![Clip::new(1, 1, 1, 0, 5_000, 0, 5_000)];

        let mut app = ClipStrikeApp::test_harness(project);
        app.timeline = app.project.to_timeline();
        app.scrub_playhead(6_000);

        assert_eq!(app.playhead_ms, 5_000);
    }

    #[test]
    fn scrub_playhead_ignores_redundant_targets() {
        let mut project = Project::new("Scrub Stable", 1920, 1080, 30.0);
        project.clips = vec![Clip::new(1, 1, 1, 0, 5_000, 0, 5_000)];

        let mut app = ClipStrikeApp::test_harness(project);
        app.timeline = app.project.to_timeline();
        app.playhead_ms = 750;
        app.is_playing = true;

        app.scrub_playhead(750);

        assert_eq!(app.playhead_ms, 750);
        assert!(app.is_playing);
    }

    #[test]
    fn playback_clips_for_playhead_prewarms_the_next_clip_on_each_track() {
        let mut project = Project::new("Playback Prewarm", 1920, 1080, 30.0);
        project.tracks = Project::default_tracks(2);
        project.clips = vec![
            Clip::new(1, 1, project.tracks[0].id, 0, 1000, 0, 1000),
            Clip::new(2, 1, project.tracks[0].id, 1100, 2100, 0, 1000),
            Clip::new(3, 1, project.tracks[1].id, 1150, 2150, 0, 1000),
        ];

        let mut app = ClipStrikeApp::test_harness(project);
        app.timeline = app.project.to_timeline();
        app.playhead_ms = 950;

        let (active_clips, prewarm_clips) = app.playback_clips_for_playhead();

        assert_eq!(
            active_clips.iter().map(|clip| clip.id).collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            prewarm_clips.iter().map(|clip| clip.id).collect::<Vec<_>>(),
            vec![2, 3]
        );
    }

    #[test]
    fn timeline_time_from_ruler_position_clamps_left_edge_and_respects_zoom() {
        let origin_x = 100.0 + TIMELINE_CONTENT_OFFSET_X;

        let at_start = ClipStrikeApp::timeline_time_from_ruler_position(
            origin_x,
            egui::pos2(origin_x, 10.0),
            0.5,
        );
        assert_eq!(at_start, 0);

        let later = ClipStrikeApp::timeline_time_from_ruler_position(
            origin_x,
            egui::pos2(origin_x + 50.0, 10.0),
            1.0,
        );
        assert_eq!(later, 500);
    }


    #[test]
    fn clip_split_time_from_pointer_offsets_by_clip_start() {
        let split_at = ClipStrikeApp::clip_split_time_from_pointer(2_000, 300.0, 350.0, 0.1);

        assert_eq!(split_at, 2_500);
    }

    fn make_test_clip() -> Clip {
        Clip::new(1, 1, 1, 0, 1000, 0, 1000)
    }

    #[test]
    fn property_accessors_map_correctly() {
        let mut clip = make_test_clip();

        // Test immutable ref
        assert_eq!(
            property_ref(&clip, AnimatedProperty::Brightness).get_value_at(0),
            1.0
        );

        // Test mutable ref
        property_mut(&mut clip, AnimatedProperty::Brightness).insert_keyframe(
            0,
            2.5,
            InterpolationMode::Linear,
        );

        assert_eq!(
            property_ref(&clip, AnimatedProperty::Brightness).get_value_at(0),
            2.5
        );
    }
}

