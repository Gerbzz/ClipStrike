use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
pub enum LutType {
    Lut1D,
    Lut3D,
}

#[derive(Debug, Clone)]
pub struct Lut {
    pub name: String,
    pub size: u32,
    pub lut_type: LutType,
    pub data: Vec<f32>, // RGBA data
}

impl Lut {
    pub fn identity_3d(size: u32) -> Self {
        let size = size.max(1);
        let mut data = Vec::with_capacity((size * size * size * 4) as usize);
        let denom = (size - 1).max(1) as f32;

        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    data.push(r as f32 / denom);
                    data.push(g as f32 / denom);
                    data.push(b as f32 / denom);
                    data.push(1.0);
                }
            }
        }

        Self {
            name: "Identity LUT".to_string(),
            size,
            lut_type: LutType::Lut3D,
            data,
        }
    }

    pub fn parse_cube(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);

        let mut size = 0;
        let mut lut_type = LutType::Lut3D;
        let mut raw_data = Vec::new();
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown LUT")
            .to_string();

        for line in reader.lines() {
            let line = line.map_err(|e| e.to_string())?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with("LUT_3D_SIZE") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    size = parts[1].parse::<u32>().map_err(|e| e.to_string())?;
                    lut_type = LutType::Lut3D;
                }
                continue;
            }

            if line.starts_with("LUT_1D_SIZE") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    size = parts[1].parse::<u32>().map_err(|e| e.to_string())?;
                    lut_type = LutType::Lut1D;
                }
                continue;
            }

            // Assume it's a data line: "R G B"
            let colors: Vec<f32> = line
                .split_whitespace()
                .filter_map(|s| s.parse::<f32>().ok())
                .collect();

            if colors.len() == 3 {
                raw_data.push(colors[0]);
                raw_data.push(colors[1]);
                raw_data.push(colors[2]);
            }
        }

        if size == 0 {
            return Err("Invalid LUT size".to_string());
        }

        // Validate data length
        let expected_floats = match lut_type {
            LutType::Lut1D => size * 3,
            LutType::Lut3D => size * size * size * 3,
        };

        if raw_data.len() != expected_floats as usize {
            return Err(format!(
                "LUT data mismatch: expected {} floats, got {}",
                expected_floats,
                raw_data.len()
            ));
        }

        // Convert to RGBA for GPU compatibility
        let mut rgba_data = Vec::with_capacity((raw_data.len() / 3) * 4);
        for chunk in raw_data.chunks_exact(3) {
            rgba_data.push(chunk[0]);
            rgba_data.push(chunk[1]);
            rgba_data.push(chunk[2]);
            rgba_data.push(1.0);
        }

        Ok(Self {
            name,
            size,
            lut_type,
            data: rgba_data,
        })
    }

    pub fn create_texture(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
        match self.lut_type {
            LutType::Lut3D => self.create_3d_texture(device, queue, self.size, &self.data),
            LutType::Lut1D => {
                // To keep the shader simple (Task 2.3), we expand 1D LUT to a 32x32x32 3D LUT
                // This provides high enough precision for most grading.
                let expanded_size = 32u32;
                let mut expanded_data = Vec::with_capacity(
                    (expanded_size * expanded_size * expanded_size * 4) as usize,
                );

                for b in 0..expanded_size {
                    for g in 0..expanded_size {
                        for r in 0..expanded_size {
                            let r_idx = ((r as f32 / (expanded_size - 1) as f32)
                                * (self.size - 1) as f32)
                                .round() as usize;
                            let g_idx = ((g as f32 / (expanded_size - 1) as f32)
                                * (self.size - 1) as f32)
                                .round() as usize;
                            let b_idx = ((b as f32 / (expanded_size - 1) as f32)
                                * (self.size - 1) as f32)
                                .round() as usize;

                            expanded_data.push(self.data[r_idx * 4]);
                            expanded_data.push(self.data[g_idx * 4 + 1]);
                            expanded_data.push(self.data[b_idx * 4 + 2]);
                            expanded_data.push(1.0);
                        }
                    }
                }
                self.create_3d_texture(device, queue, expanded_size, &expanded_data)
            }
        }
    }

    fn create_3d_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: u32,
        rgba_data: &[f32],
    ) -> wgpu::Texture {
        let texture_size = wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: size,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("LUT Texture: {}", self.name)),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let rgba_bytes: Vec<u8> = rgba_data
            .iter()
            .map(|component| (component.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect();

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&rgba_bytes),
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * size),
                rows_per_image: Some(size),
            },
            texture_size,
        );

        texture
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_3d_lut_generates_expected_rgba_gradient() {
        let lut = Lut::identity_3d(2);

        assert_eq!(lut.name, "Identity LUT");
        assert_eq!(lut.size, 2);
        assert!(matches!(lut.lut_type, LutType::Lut3D));
        assert_eq!(lut.data.len(), 32);
        assert_eq!(&lut.data[0..4], &[0.0, 0.0, 0.0, 1.0]);
        assert_eq!(&lut.data[28..32], &[1.0, 1.0, 1.0, 1.0]);
    }
}
