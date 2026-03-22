use std::sync::Arc;
use tokio::sync::oneshot;

use eframe::egui;
use wgpu;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HANDLE;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;

/// Represents a single reusable texture frame on the GPU.
#[allow(dead_code)]
pub struct FrameTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub native_texture: Option<wgpu::Texture>,
    pub native_view: Option<wgpu::TextureView>,
    pub index: usize,
    pub ready: bool,
    pub clip_id: Option<usize>,
    pub external_texture_id: Option<u32>,
    pub native_d3d12_shared_handle: Option<usize>,
    pub egui_id: Option<egui::TextureId>,
}

impl FrameTexture {
    pub fn preview_view(&self) -> &wgpu::TextureView {
        self.native_view.as_ref().unwrap_or(&self.view)
    }

    pub fn has_native_preview(&self) -> bool {
        self.native_view.is_some()
    }

    fn clear_preview_metadata(&mut self) {
        self.native_texture = None;
        self.native_view = None;
        self.native_d3d12_shared_handle = None;
    }
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
    warned_native_import_failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameUploadRequest {
    pub clip_id: usize,
    pub timestamp_ms: i64,
    pub external_texture_id: Option<u32>,
    pub native_d3d12_shared_handle: Option<usize>,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSourceKind {
    NativeTexture,
    ExternalTexture(u32),
    ImportedNativeTexture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameUploadState {
    ready: bool,
    clip_id: Option<usize>,
    external_texture_id: Option<u32>,
}

pub fn frame_source_kind(
    external_texture_id: Option<u32>,
    has_native_preview: bool,
) -> FrameSourceKind {
    if has_native_preview {
        FrameSourceKind::ImportedNativeTexture
    } else if let Some(texture_id) = external_texture_id {
        FrameSourceKind::ExternalTexture(texture_id)
    } else {
        FrameSourceKind::NativeTexture
    }
}

fn frame_upload_state_from_request(request: &FrameUploadRequest) -> FrameUploadState {
    FrameUploadState {
        ready: true,
        clip_id: Some(request.clip_id),
        external_texture_id: request.external_texture_id,
    }
}

#[cfg(target_os = "windows")]
fn import_native_d3d12_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    shared_handle: usize,
) -> Result<(wgpu::Texture, wgpu::TextureView), String> {
    let Some(result) = (unsafe {
        device.as_hal::<wgpu::hal::api::Dx12, _, _>(|maybe_hal_device| {
            let Some(hal_device) = maybe_hal_device else {
                return Err(String::from("WGPU device is not backed by DX12"));
            };

            let handle = HANDLE(shared_handle as *mut core::ffi::c_void);
            let resource = {
                let mut resource = None::<ID3D12Resource>;
                hal_device
                    .raw_device()
                    .OpenSharedHandle(handle, &mut resource)
                    .map_err(|err| format!("Failed to open D3D12 shared handle: {err:?}"))?;
                resource.ok_or_else(|| String::from("OpenSharedHandle returned no resource"))?
            };

            let hal_texture = wgpu::hal::dx12::Device::texture_from_raw(
                resource,
                format,
                wgpu::TextureDimension::D2,
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                1,
                1,
            );

            let texture = device.create_texture_from_hal::<wgpu::hal::api::Dx12>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some("Imported Native Frame Texture"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
            );

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            Ok((texture, view))
        })
    }) else {
        return Err(String::from(
            "WGPU device does not expose a DX12 backend for native import",
        ));
    };

    result
}

pub fn copy_readback_pixels_into(
    mapped: &[u8],
    width: u32,
    height: u32,
    padded_bytes_per_row: usize,
    pixels: &mut Vec<u8>,
) {
    let bytes_per_pixel = 4usize;
    let unpadded_bytes_per_row = width as usize * bytes_per_pixel;
    let total_bytes = unpadded_bytes_per_row * height as usize;

    pixels.clear();
    pixels.resize(total_bytes, 0);

    if padded_bytes_per_row == unpadded_bytes_per_row {
        pixels.copy_from_slice(mapped);
        return;
    }

    for row in 0..height as usize {
        let src_offset = row * padded_bytes_per_row;
        let dst_offset = row * unpadded_bytes_per_row;
        let src = &mapped[src_offset..src_offset + unpadded_bytes_per_row];
        let dst = &mut pixels[dst_offset..dst_offset + unpadded_bytes_per_row];
        dst.copy_from_slice(src);
    }
}

pub fn read_texture_rgba_into(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    pixels: &mut Vec<u8>,
) -> Result<(), String> {
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

    #[cfg(target_arch = "wasm32")]
    {
        pixels.clear();
        pixels.resize((width * height * bytes_per_pixel) as usize, 0);
        return Ok(());
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let slice = output_buffer.slice(..);
        let (sender, receiver) = oneshot::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device.poll(wgpu::Maintain::Wait);

        wait_for_map_result(receiver)?;

        let mapped = slice.get_mapped_range();
        copy_readback_pixels_into(
            &mapped,
            width,
            height,
            padded_bytes_per_row as usize,
            pixels,
        );
        Ok(())
    }
}

pub fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let mut pixels = Vec::new();
    read_texture_rgba_into(device, queue, texture, width, height, &mut pixels)?;
    Ok(pixels)
}

fn wait_for_map_result(
    receiver: oneshot::Receiver<Result<(), wgpu::BufferAsyncError>>,
) -> Result<(), String> {
    match receiver.blocking_recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(format!("Failed to map readback buffer: {err:?}")),
        Err(_) => Err(String::from(
            "Failed waiting for readback buffer: sender dropped",
        )),
    }
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
                native_texture: None,
                native_view: None,
                index: i,
                ready: false,
                clip_id: None,
                external_texture_id: None,
                native_d3d12_shared_handle: None,
                egui_id: None,
            });
        }

        Self {
            device,
            format,
            width,
            height,
            pool,
            warned_native_import_failed: false,
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
        recycled.external_texture_id = None;
        recycled.clear_preview_metadata();
        Some(recycled)
    }

    pub fn frame_for_clip(&self, clip_id: usize) -> Option<&FrameTexture> {
        self.pool
            .iter()
            .find(|frame| frame.ready && frame.clip_id == Some(clip_id))
    }

    pub fn frame_source_kind(frame: &FrameTexture) -> FrameSourceKind {
        frame_source_kind(frame.external_texture_id, frame.has_native_preview())
    }

    pub fn get_frames(&self) -> &[FrameTexture] {
        &self.pool
    }

    /// Update a texture from an external GStreamer handle using wgpu_hal
    pub fn update_from_external_id(
        &mut self,
        clip_id: usize,
        external_id: u32,
    ) -> Result<(), String> {
        let frame = self.acquire_for_clip(clip_id).ok_or("No frame available")?;
        frame.ready = true;
        frame.clip_id = Some(clip_id);
        frame.external_texture_id = Some(external_id);
        frame.clear_preview_metadata();
        Ok(())
    }

    pub fn apply_frame_upload_request(
        &mut self,
        queue: &wgpu::Queue,
        request: FrameUploadRequest,
    ) -> Result<bool, String> {
        let width = self.width;
        let height = self.height;
        let device = Arc::clone(&self.device);
        let format = self.format;

        let state = frame_upload_state_from_request(&request);
        let mut imported_native_preview = false;
        let mut native_import_err = None;

        {
            let frame = self
                .acquire_for_clip(request.clip_id)
                .ok_or_else(|| format!("No frame available for clip {}", request.clip_id))?;

            if !request.pixels.is_empty() {
                let bytes_per_row = width * 4;
                if request.pixels.len() != (width as usize * height as usize * 4) {
                    return Err(format!(
                        "Unexpected pixel payload size for clip {}: got {}, expected {}",
                        request.clip_id,
                        request.pixels.len(),
                        width as usize * height as usize * 4,
                    ));
                }
                queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &frame.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &request.pixels,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(bytes_per_row),
                        rows_per_image: Some(height),
                    },
                    wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                );
            }

            if let Some(shared_handle) = request.native_d3d12_shared_handle {
                match import_native_d3d12_texture(&device, width, height, format, shared_handle) {
                    Ok((native_texture, native_view)) => {
                        frame.native_texture = Some(native_texture);
                        frame.native_view = Some(native_view);
                        frame.native_d3d12_shared_handle = Some(shared_handle);
                        imported_native_preview = true;
                    }
                    Err(err) => {
                        native_import_err = Some(err);
                        frame.clear_preview_metadata();
                    }
                }
            } else {
                frame.clear_preview_metadata();
            }

            frame.ready = state.ready;
            frame.clip_id = state.clip_id;
            frame.external_texture_id = state.external_texture_id;
        }

        if let Some(err) = native_import_err {
            if !self.warned_native_import_failed {
                log::warn!(
                    "Falling back to CPU frame for clip {} after native import failed: {}. This usually happens if the WGPU backend is not DX12 on Windows. Subsequent failures for this session will be silenced.",
                    request.clip_id,
                    err
                );
                self.warned_native_import_failed = true;
            }
        }

        Ok(imported_native_preview)
    }

    /// Mark a texture as finished playing and available for the decoder again.
    pub fn release(&mut self, index: usize) {
        if let Some(frame) = self.pool.get_mut(index) {
            frame.ready = false;
            frame.clip_id = None;
            frame.external_texture_id = None;
            frame.clear_preview_metadata();
        }
    }

    pub fn release_clip_frames(&mut self, clip_id: usize) {
        for frame in &mut self.pool {
            if frame.clip_id == Some(clip_id) {
                frame.ready = false;
                frame.clip_id = None;
                frame.external_texture_id = None;
                frame.clear_preview_metadata();
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
    fn frame_upload_request_type_preserves_clip_identity() {
        let request = super::FrameUploadRequest {
            clip_id: 42,
            timestamp_ms: 1337,
            external_texture_id: Some(7),
            native_d3d12_shared_handle: None,
            pixels: vec![1, 2, 3, 4],
        };

        assert_eq!(request.clip_id, 42);
        assert_eq!(request.timestamp_ms, 1337);
        assert_eq!(request.external_texture_id, Some(7));
        assert_eq!(request.pixels, vec![1, 2, 3, 4]);
    }

    #[test]
    fn frame_source_kind_reports_imported_native_preview_metadata() {
        assert_eq!(
            super::frame_source_kind(None, true),
            super::FrameSourceKind::ImportedNativeTexture
        );
    }

    #[test]
    fn frame_upload_state_from_request_keeps_external_texture_id() {
        let request = super::FrameUploadRequest {
            clip_id: 7,
            timestamp_ms: 99,
            external_texture_id: Some(12),
            native_d3d12_shared_handle: None,
            pixels: Vec::new(),
        };

        let state = super::frame_upload_state_from_request(&request);
        assert!(state.ready);
        assert_eq!(state.clip_id, Some(7));
        assert_eq!(state.external_texture_id, Some(12));
    }

    #[test]
    fn frame_source_kind_reports_external_texture_metadata() {
        assert_eq!(
            super::frame_source_kind(Some(44), false),
            super::FrameSourceKind::ExternalTexture(44)
        );
        assert_eq!(
            super::frame_source_kind(None, false),
            super::FrameSourceKind::NativeTexture
        );
        assert_eq!(
            super::frame_source_kind(None, true),
            super::FrameSourceKind::ImportedNativeTexture
        );
    }

    #[test]
    fn wait_for_map_result_accepts_success_notification() {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        sender.send(Ok(())).expect("send map result");

        let result = super::wait_for_map_result(receiver);
        assert!(result.is_ok(), "expected successful map wait");
    }

    #[test]
    fn copy_readback_pixels_into_reuses_existing_buffer_for_tight_and_padded_rows() {
        let tight = vec![1_u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let padded = vec![
            21_u8, 22, 23, 24, 25, 26, 27, 28, 200, 201, 202, 203, 29, 30, 31, 32, 33, 34, 35, 36,
            204, 205, 206, 207,
        ];
        let mut pixels = Vec::new();

        super::copy_readback_pixels_into(&tight, 2, 2, 8, &mut pixels);
        let first_ptr = pixels.as_ptr();
        assert_eq!(pixels, tight);

        super::copy_readback_pixels_into(&padded, 2, 2, 12, &mut pixels);
        assert_eq!(
            pixels,
            vec![
                21_u8, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36
            ]
        );
        assert_eq!(
            pixels.as_ptr(),
            first_ptr,
            "expected the buffer allocation to be reused"
        );
    }
}
