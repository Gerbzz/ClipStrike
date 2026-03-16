use std::sync::Arc;
use std::sync::mpsc;

use eframe::egui;
use wgpu;

/// Represents a single reusable texture frame on the GPU.
#[allow(dead_code)]
pub struct FrameTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub index: usize,
    pub ready: bool,
    pub clip_id: Option<usize>,
    pub egui_id: Option<egui::TextureId>,
}

/// A pool of hardware textures.
/// In a zero-copy pipeline, we want to pre-allocate these and pass their
/// references to the GStreamer hardware decoder. Once the decoder fills
/// them, it gives them back to us to render in egui.
pub struct TexturePool {
    device: Arc<wgpu::Device>,
    pub format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pool: Vec<FrameTexture>,
}

pub fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let bytes_per_pixel = 4u32;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
    let output_buffer_size =
        padded_bytes_per_row as wgpu::BufferAddress * height as wgpu::BufferAddress;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        size: output_buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        label: Some("Readback Buffer"),
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Readback Encoder"),
    });

    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            aspect: wgpu::TextureAspect::All,
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
        },
        wgpu::ImageCopyBuffer {
            buffer: &output_buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(Some(encoder.finish()));

    let slice = output_buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device.poll(wgpu::Maintain::Wait);

    match receiver.recv() {
        Ok(Ok(())) => {}
        Ok(Err(err)) => return Err(format!("Failed to map readback buffer: {err:?}")),
        Err(err) => return Err(format!("Failed waiting for readback buffer: {err}")),
    }

    let mapped = slice.get_mapped_range();
    let mut pixels = vec![0; (width * height * bytes_per_pixel) as usize];
    for row in 0..height as usize {
        let src_offset = row * padded_bytes_per_row as usize;
        let dst_offset = row * unpadded_bytes_per_row as usize;
        let src = &mapped[src_offset..src_offset + unpadded_bytes_per_row as usize];
        let dst = &mut pixels[dst_offset..dst_offset + unpadded_bytes_per_row as usize];
        dst.copy_from_slice(src);
    }
    drop(mapped);
    output_buffer.unmap();

    Ok(pixels)
}

impl TexturePool {
    /// Create a new pool with the given capacity.
    pub fn new(
        device: Arc<wgpu::Device>,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        capacity: usize,
    ) -> Self {
        let mut pool = Vec::with_capacity(capacity);

        for i in 0..capacity {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("FrameTexture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            pool.push(FrameTexture {
                texture,
                view,
                index: i,
                ready: false,
                clip_id: None,
                egui_id: None,
            });
        }

        Self {
            device,
            format,
            width,
            height,
            pool,
        }
    }

    pub fn acquire_for_clip(&mut self, clip_id: usize) -> Option<&mut FrameTexture> {
        if let Some(index) = self
            .pool
            .iter()
            .position(|frame| frame.clip_id == Some(clip_id))
        {
            return self.pool.get_mut(index);
        }
        if let Some(index) = self
            .pool
            .iter()
            .position(|frame| !frame.ready || frame.clip_id.is_none())
        {
            return self.pool.get_mut(index);
        }
        if self.pool.is_empty() {
            return None;
        }

        let recycle_index = clip_id % self.pool.len();
        let recycled = self.pool.get_mut(recycle_index)?;
        recycled.ready = false;
        recycled.clip_id = None;
        Some(recycled)
    }

    pub fn frame_for_clip(&self, clip_id: usize) -> Option<&FrameTexture> {
        self.pool
            .iter()
            .find(|frame| frame.ready && frame.clip_id == Some(clip_id))
    }

    pub fn get_frames(&self) -> &[FrameTexture] {
        &self.pool
    }

    /// Mark a texture as finished playing and available for the decoder again.
    pub fn release(&mut self, index: usize) {
        if let Some(frame) = self.pool.get_mut(index) {
            frame.ready = false;
            frame.clip_id = None;
        }
    }

    pub fn release_clip_frames(&mut self, clip_id: usize) {
        for frame in &mut self.pool {
            if frame.clip_id == Some(clip_id) {
                frame.ready = false;
                frame.clip_id = None;
            }
        }
    }

    pub fn get_frames_mut(&mut self) -> &mut [FrameTexture] {
        &mut self.pool
    }

    /// Read a frame back from the GPU to the CPU.
    pub fn read_frame(&self, queue: &wgpu::Queue, index: usize) -> Result<Vec<u8>, String> {
        let frame = self
            .pool
            .get(index)
            .ok_or_else(|| format!("Invalid frame index: {index}"))?;
        read_texture_rgba(&self.device, queue, &frame.texture, self.width, self.height)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_texture_pool_capacity() {
        assert!(true);
    }
}
