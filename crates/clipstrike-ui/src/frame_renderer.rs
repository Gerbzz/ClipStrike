use clipstrike_core::timeline::Clip;
use clipstrike_render::fx::{ComposeUniforms, CompositorPipeline, FxPipeline, FxUniforms};
use clipstrike_render::gpu::{read_texture_rgba_into, FrameUploadRequest, TexturePool};
use std::sync::Arc;

pub(crate) struct RenderLayer<'a> {
    pub clip: &'a Clip,
    pub source_ms: Option<i64>,
    pub transform: [f32; 16],
}

pub(crate) struct FrameRenderer {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub texture_pool: TexturePool,
    pub fx_pipeline: FxPipeline,
    pub compositor_pipeline: CompositorPipeline,
    pub fx_output_texture: wgpu::Texture,
    pub fx_output_view: wgpu::TextureView,
    pub accumulator_texture: wgpu::Texture,
    pub accumulator_view: wgpu::TextureView,
    pub scratch_texture: wgpu::Texture,
    pub scratch_view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl FrameRenderer {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        width: u32,
        height: u32,
    ) -> Self {
        let texture_pool = TexturePool::new(
            device.clone(),
            wgpu::TextureFormat::Rgba8Unorm,
            width,
            height,
            10,
        );
        let fx_output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("FxPreviewTexture"),
            size: wgpu::Extent3d {
                width,
                height,
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
        let accumulator_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Compositor Accumulator"),
            size: wgpu::Extent3d {
                width,
                height,
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
        let accumulator_view =
            accumulator_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let scratch_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Compositor Scratch"),
            size: wgpu::Extent3d {
                width,
                height,
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        Self {
            device,
            queue,
            texture_pool,
            fx_pipeline,
            compositor_pipeline,
            fx_output_texture,
            fx_output_view,
            accumulator_texture,
            accumulator_view,
            scratch_texture,
            scratch_view,
            sampler,
        }
    }

    pub fn upload(&mut self, request: FrameUploadRequest) -> Result<bool, String> {
        self.texture_pool
            .apply_frame_upload_request(&self.queue, request)
    }

    pub fn render_at<'a>(
        &self,
        layers: impl IntoIterator<Item = RenderLayer<'a>>,
        playhead_ms: i64,
        max_distance_ms: i64,
        strict_export: bool,
    ) -> bool {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Clear Accumulator"),
            });
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.accumulator_view,
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
        self.queue.submit(Some(encoder.finish()));

        let mut rendered = false;
        for layer in layers {
            let clip = layer.clip;
            let frame = if strict_export {
                match layer.source_ms {
                    Some(source_ms) => self
                        .texture_pool
                        .frame_for_clip_covering_ns(clip.id, source_ms.max(0) as u64 * 1_000_000),
                    None => self.texture_pool.frame_for_clip(clip.id),
                }
            } else {
                self.texture_pool
                    .frame_for_clip_at(clip.id, layer.source_ms, max_distance_ms)
            };
            let Some(frame) = frame else {
                continue;
            };
            let fx_params = FxUniforms {
                brightness: clip.brightness.get_value_at(playhead_ms),
                contrast: clip.contrast.get_value_at(playhead_ms),
                saturation: clip.saturation.get_value_at(playhead_ms),
                grayscale: clip.grayscale.get_value_at(playhead_ms),
                blur_radius: clip.blur_radius.get_value_at(playhead_ms),
                alpha: clip.opacity.get_value_at(playhead_ms),
                transform: layer.transform,
                ..Default::default()
            };
            self.fx_pipeline.run(
                &self.device,
                &self.queue,
                frame.preview_view(),
                &self.fx_output_view,
                None,
                &self.sampler,
                fx_params,
            );
            let compose_params = ComposeUniforms {
                blend_mode: blend_mode_to_u32(clip.blend_mode),
                opacity: 1.0,
                watermark_enabled: 0,
                watermark_opacity: 1.0,
                watermark_rect: [0.0, 0.0, 0.0, 0.0],
            };
            self.compositor_pipeline.run(
                &self.device,
                &self.queue,
                &self.fx_output_view,
                &self.accumulator_view,
                &self.scratch_view,
                &self.fx_output_view,
                &self.sampler,
                compose_params,
            );

            let mut copy_encoder =
                self.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Copy Scratch to Accum"),
                    });
            copy_encoder.copy_texture_to_texture(
                wgpu::ImageCopyTexture {
                    texture: &self.scratch_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::ImageCopyTexture {
                    texture: &self.accumulator_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.texture_pool.width,
                    height: self.texture_pool.height,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(Some(copy_encoder.finish()));
            rendered = true;
        }
        rendered
    }

    pub fn read_rgba_into(&self, pixels: &mut Vec<u8>) -> Result<(u32, u32), String> {
        let width = self.texture_pool.width;
        let height = self.texture_pool.height;
        pixels.clear();
        read_texture_rgba_into(
            &self.device,
            &self.queue,
            &self.accumulator_texture,
            width,
            height,
            pixels,
        )?;
        Ok((width, height))
    }
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
        _ => 0,
    }
}
