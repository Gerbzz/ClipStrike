#[cfg(feature = "ai-features")]
use opencv::{
    core::{Rect as CvRect, Size},
    prelude::*,
    video::TrackerKCF,
};

use clipstrike_core::effects::DetectedObject;
pub struct AnalysisResult {
    pub points: Vec<(i64, f32, f32, f32, f32)>, // time_ms, x, y, w, h
}

#[cfg(feature = "ai-features")]
pub fn track_object(
    uri: &str,
    start_ms: i64,
    end_ms: i64,
    initial_region: (f32, f32, f32, f32), // norm x, y, w, h
) -> Result<AnalysisResult, String> {
    // This function will use GStreamer to pull frames and OpenCV to track.
    // For Task 2.1, we implement the structure.

    // 1. Initialize GStreamer pipeline to pull frames from start_ms to end_ms
    // 2. Initialize OpenCV KCF Tracker with initial_region
    // 3. For each frame:
    //    a. Convert GstBuffer to cv::Mat
    //    b. tracker.update(frame)
    //    c. Store results

    // Mock implementation for now to ensure pipeline works
    let mut points = Vec::new();
    let steps = 10;
    for i in 0..=steps {
        let time = start_ms + (end_ms - start_ms) * i / steps;
        points.push((
            time,
            initial_region.0,
            initial_region.1,
            initial_region.2,
            initial_region.3,
        ));
    }

    Ok(AnalysisResult { points })
}

#[cfg(not(feature = "ai-features"))]
pub fn track_object(
    _uri: &str,
    _start_ms: i64,
    _end_ms: i64,
    _initial_region: (f32, f32, f32, f32),
) -> Result<AnalysisResult, String> {
    Err("AI features are not enabled. Recompile with --features ai-features".to_string())
}

pub fn detect_preroll_objects(
    frame_rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<DetectedObject>, String> {
    if width == 0 || height == 0 {
        return Err("Frame dimensions must be non-zero".to_string());
    }

    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixel_count| pixel_count.checked_mul(4))
        .ok_or_else(|| "Frame dimensions are too large".to_string())?;
    if frame_rgba.len() != expected_len {
        return Err(format!(
            "Frame buffer length {} did not match {}x{} RGBA",
            frame_rgba.len(),
            width,
            height
        ));
    }

    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut bright_pixels = 0usize;
    let total_pixels = (width as usize) * (height as usize);

    for y in 0..height {
        for x in 0..width {
            let idx = ((y as usize) * (width as usize) + (x as usize)) * 4;
            let r = frame_rgba[idx] as u32;
            let g = frame_rgba[idx + 1] as u32;
            let b = frame_rgba[idx + 2] as u32;
            let luma = (r + g + b) / 3;

            if luma >= 200 {
                bright_pixels += 1;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    if bright_pixels == 0 {
        return Ok(Vec::new());
    }

    let bbox_width = max_x - min_x + 1;
    let bbox_height = max_y - min_y + 1;
    let bbox_area = (bbox_width as usize) * (bbox_height as usize);
    let fill_ratio = bright_pixels as f32 / bbox_area as f32;
    let pixel_ratio = bright_pixels as f32 / total_pixels as f32;
    let confidence = fill_ratio.max(pixel_ratio * 4.0).clamp(0.05, 0.99);

    Ok(vec![DetectedObject {
        label: "Preroll Object".to_string(),
        confidence,
        x: min_x as f32 / width as f32,
        y: min_y as f32 / height as f32,
        width: bbox_width as f32 / width as f32,
        height: bbox_height as f32 / height as f32,
    }])
}

#[cfg(test)]
mod tests {
    use super::detect_preroll_objects;

    #[test]
    fn detect_preroll_objects_finds_prominent_blob() {
        let width = 8;
        let height = 8;
        let mut frame = vec![0u8; width * height * 4];

        for y in 2..6 {
            for x in 3..7 {
                let idx = (y * width + x) * 4;
                frame[idx..idx + 4].copy_from_slice(&[240, 240, 240, 255]);
            }
        }

        let detections = detect_preroll_objects(&frame, width as u32, height as u32)
            .expect("detection should succeed");

        assert_eq!(detections.len(), 1);
        let object = &detections[0];
        assert_eq!(object.label, "Preroll Object");
        assert!(object.confidence > 0.5);
        assert!((object.x - 3.0 / 8.0).abs() < 0.05);
        assert!((object.y - 2.0 / 8.0).abs() < 0.05);
        assert!((object.width - 4.0 / 8.0).abs() < 0.05);
        assert!((object.height - 4.0 / 8.0).abs() < 0.05);
    }
}
