use glam::{Mat4, Vec3};
use wgpu;

/// Represents the effect properties that will be sent to the compute shader.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FxUniforms {
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub alpha: f32,
    pub grayscale: f32,
    pub blur_radius: f32,
    pub pixelate_size: f32,
    pub color_tint: [f32; 3],
    pub transform: [f32; 16], // 4x4 Matrix
}

impl Default for FxUniforms {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            contrast: 1.0,
            saturation: 1.0,
            alpha: 1.0,
            grayscale: 0.0,
            blur_radius: 0.0,
            pixelate_size: 1.0,
            color_tint: [1.0, 1.0, 1.0],
            transform: Mat4::IDENTITY.to_cols_array(),
        }
    }
}

/// The architecture for running compute shaders on video frames.
pub struct FxPipeline {
    pub compute_pipeline: wgpu::ComputePipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl FxPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        // Load the WGSL compute shader
        let shader_src = include_str!("shaders/fx.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fx Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        // Define the layout: 1 input texture, 1 output texture, 1 uniform buffer
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Fx Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Fx Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Fx Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            compute_pipeline,
            bind_group_layout,
        }
    }

    pub fn run(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        input_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
        params: FxUniforms,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Fx Encoder"),
        });

        // Create uniform buffer
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fx Uniform Buffer"),
            size: std::mem::size_of::<FxUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&[params]));

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Fx Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(output_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Fx Pass"),
                ..Default::default()
            });
            compute_pass.set_pipeline(&self.compute_pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            // Assuming 1920x1080 for now, divided by workgroup size 16
            compute_pass.dispatch_workgroups(120, 68, 1);
        }

        queue.submit(Some(encoder.finish()));
    }
}

/// Represents the parameters for the compositor shader.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ComposeUniforms {
    pub blend_mode: u32,
    pub opacity: f32,
}

/// Handles blending of two layers using professional blend modes.
pub struct CompositorPipeline {
    pub compute_pipeline: wgpu::ComputePipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl CompositorPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader_src = include_str!("shaders/compositor.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Compositor Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Compositor Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Compositor Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Compositor Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            compute_pipeline,
            bind_group_layout,
        }
    }

    pub fn run(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        top_view: &wgpu::TextureView,
        bottom_view: &wgpu::TextureView,
        output_view: &wgpu::TextureView,
        params: ComposeUniforms,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Compositor Encoder"),
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Compositor Uniform Buffer"),
            size: std::mem::size_of::<ComposeUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        queue.write_buffer(&uniform_buffer, 0, bytemuck::cast_slice(&[params]));

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compositor Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(top_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(bottom_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(output_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Compositor Pass"),
                ..Default::default()
            });
            compute_pass.set_pipeline(&self.compute_pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(120, 68, 1);
        }

        queue.submit(Some(encoder.finish()));
    }
}

/// Task 4.3: Keyframe Interpolation
/// Uses Bezier/Linear interpolation to calculate values over time.
pub struct KeyframeEngine;

impl KeyframeEngine {
    pub fn interpolate(start_val: f32, end_val: f32, progress: f32) -> f32 {
        // Simple linear interpolation for now
        // A full implementation would use bezier curve logic
        start_val + (end_val - start_val) * progress.clamp(0.0, 1.0)
    }

    pub fn calculate_transform(scale: Vec3, translation: Vec3) -> Mat4 {
        Mat4::from_scale_rotation_translation(scale, glam::Quat::IDENTITY, translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn test_fx_uniforms_default() {
        let uniforms = FxUniforms::default();
        assert_eq!(uniforms.brightness, 1.0);
        assert_eq!(uniforms.contrast, 1.0);
        assert_eq!(uniforms.saturation, 1.0);
        assert_eq!(uniforms.alpha, 1.0);
        assert_eq!(uniforms.grayscale, 0.0);
        assert_eq!(uniforms.blur_radius, 0.0);
        assert_eq!(uniforms.transform, Mat4::IDENTITY.to_cols_array());
    }

    #[test]
    fn test_keyframe_interpolate() {
        assert_eq!(KeyframeEngine::interpolate(0.0, 100.0, 0.0), 0.0);
        assert_eq!(KeyframeEngine::interpolate(0.0, 100.0, 0.5), 50.0);
        assert_eq!(KeyframeEngine::interpolate(0.0, 100.0, 1.0), 100.0);
        
        // Progress clamping
        assert_eq!(KeyframeEngine::interpolate(0.0, 100.0, 1.5), 100.0);
        assert_eq!(KeyframeEngine::interpolate(0.0, 100.0, -0.5), 0.0);
    }

    #[test]
    fn test_calculate_transform() {
        let scale = Vec3::new(2.0, 2.0, 1.0);
        let translation = Vec3::new(10.0, 20.0, 0.0);
        let mat = KeyframeEngine::calculate_transform(scale, translation);
        
        let cols = mat.to_cols_array();
        assert_eq!(cols[0], 2.0); // scale x
        assert_eq!(cols[5], 2.0); // scale y
        assert_eq!(cols[12], 10.0); // translate x
        assert_eq!(cols[13], 20.0); // translate y
    }
}
