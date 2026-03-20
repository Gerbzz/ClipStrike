use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipMetadata {
    pub duration: Option<Duration>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps_num: Option<i32>,
    pub fps_den: Option<i32>,
}

#[allow(dead_code)]
fn copy_readback_pixels_into(
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

#[cfg(all(test, feature = "gstreamer-backend"))]
fn copy_readback_pixels(
    mapped: &[u8],
    width: u32,
    height: u32,
    padded_bytes_per_row: usize,
) -> Vec<u8> {
    let mut pixels = Vec::new();
    copy_readback_pixels_into(mapped, width, height, padded_bytes_per_row, &mut pixels);
    pixels
}

#[cfg(feature = "gstreamer-backend")]
pub fn probe_metadata_sync(uri: &str) -> Result<ClipMetadata, String> {
    use gstreamer as gst;
    use gstreamer_pbutils as gst_pbutils;

    if let Err(e) = gst::init() {
        return Err(format!("Failed to initialize GStreamer: {}", e));
    }

    let timeout = gst::ClockTime::from_seconds(5);
    let discoverer = gst_pbutils::Discoverer::new(timeout)
        .map_err(|e| format!("Failed to create Discoverer: {}", e))?;
    let info = discoverer
        .discover_uri(uri)
        .map_err(|e| format!("Failed to discover URI ({}): {}", uri, e))?;

    let duration = info
        .duration()
        .map(|d: gst::ClockTime| Duration::from_nanos(d.nseconds()));

    let mut width = None;
    let mut height = None;
    let mut fps_num = None;
    let mut fps_den = None;

    if let Some(video_stream_info) = info.video_streams().first() {
        width = Some(video_stream_info.width() as u32);
        height = Some(video_stream_info.height() as u32);
        let fps = video_stream_info.framerate();
        fps_num = Some(fps.numer());
        fps_den = Some(fps.denom());
    }

    Ok(ClipMetadata {
        duration,
        width,
        height,
        fps_num,
        fps_den,
    })
}

#[cfg(feature = "gstreamer-backend")]
pub fn generate_thumbnail_sync_into(
    uri: &str,
    time_ms: i64,
    pixels: &mut Vec<u8>,
) -> Result<(u32, u32), String> {
    use gstreamer as gst;
    use gstreamer::prelude::*;
    use gstreamer_app as gst_app;

    if let Err(e) = gst::init() {
        return Err(format!("Failed to init GStreamer: {}", e));
    }

    // A simple pipeline to decode, scale down, and extract a single RGBA frame.
    // 256x144 is a good thumbnail size (16:9).
    let pipeline_str = format!(
        "uridecodebin uri=\"{}\" ! videoconvert ! videoscale ! video/x-raw,format=RGBA,width=256,height=144 ! appsink name=sink drop=true max-buffers=1",
        uri
    );

    let pipeline = gst::parse::launch(&pipeline_str)
        .map_err(|e| format!("Failed to parse pipeline: {}", e))?
        .downcast::<gst::Pipeline>()
        .map_err(|_| "Failed to cast to Pipeline")?;

    let sink = pipeline.by_name("sink").ok_or("Failed to get appsink")?;
    let appsink = sink
        .downcast::<gst_app::AppSink>()
        .map_err(|_| "Not an AppSink")?;

    // Pause the pipeline to allow seeking
    pipeline
        .set_state(gst::State::Paused)
        .map_err(|e| format!("Failed to set state to Paused: {}", e))?;

    // Wait for the pipeline to preroll
    let state_res = pipeline.state(gst::ClockTime::from_seconds(5));
    if state_res.0.is_err() {
        let _ = pipeline.set_state(gst::State::Null);
        return Err("Pipeline failed to preroll".to_string());
    }

    // Seek to the requested time
    let seek_time = gst::ClockTime::from_mseconds(time_ms as u64);
    if pipeline
        .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, seek_time)
        .is_err()
    {
        log::warn!(
            "Failed to seek to {} ms for thumbnail, using current frame",
            time_ms
        );
    }

    // Pull the sample
    let sample = appsink
        .pull_preroll()
        .map_err(|_| "Failed to pull preroll sample")?;
    let buffer = sample.buffer().ok_or("Sample has no buffer")?;

    let caps = sample.caps().ok_or("Sample has no caps")?;
    let s = caps.structure(0).ok_or("Caps have no structure")?;
    let width = s.get::<i32>("width").map_err(|_| "No width in caps")? as u32;
    let height = s.get::<i32>("height").map_err(|_| "No height in caps")? as u32;

    let map = buffer.map_readable().map_err(|_| "Failed to map buffer")?;
    copy_readback_pixels_into(&map, width, height, (width as usize) * 4, pixels);

    let _ = pipeline.set_state(gst::State::Null);

    Ok((width, height))
}

#[cfg(feature = "gstreamer-backend")]
pub fn generate_thumbnail_sync(uri: &str, time_ms: i64) -> Result<(Vec<u8>, u32, u32), String> {
    let mut pixels = Vec::new();
    let (width, height) = generate_thumbnail_sync_into(uri, time_ms, &mut pixels)?;
    Ok((pixels, width, height))
}

#[cfg(not(feature = "gstreamer-backend"))]
pub fn generate_thumbnail_sync(uri: &str, _time_ms: i64) -> Result<(Vec<u8>, u32, u32), String> {
    Err(format!(
        "Thumbnail generation unavailable without GStreamer backend: {}",
        uri
    ))
}

#[cfg(feature = "gstreamer-backend")]
pub async fn generate_thumbnail(uri: String, time_ms: i64) -> Result<(Vec<u8>, u32, u32), String> {
    use tokio::task;
    task::spawn_blocking(move || generate_thumbnail_sync(&uri, time_ms))
        .await
        .map_err(|e| format!("Spawn blocking failed: {}", e))?
}

#[cfg(not(feature = "gstreamer-backend"))]
pub fn probe_metadata_sync(uri: &str) -> Result<ClipMetadata, String> {
    Err(format!(
        "Media probing is unavailable because the GStreamer backend is disabled: {}",
        uri
    ))
}

#[cfg(feature = "gstreamer-backend")]
pub async fn probe_metadata(uri: String) -> Result<ClipMetadata, String> {
    use tokio::task;

    task::spawn_blocking(move || probe_metadata_sync(&uri))
        .await
        .map_err(|e| format!("Spawn blocking failed: {}", e))?
}

#[cfg(not(feature = "gstreamer-backend"))]
pub async fn probe_metadata(uri: String) -> Result<ClipMetadata, String> {
    probe_metadata_sync(&uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_sync_invalid_uri() {
        let uri = "file:///non/existent/file.mp4";
        let result = probe_metadata_sync(uri);
        assert!(result.is_err());
    }

    #[cfg(feature = "gstreamer-backend")]
    #[test]
    fn contiguous_readback_pixels_return_exact_buffer_when_rows_are_tight() {
        let mapped = vec![1_u8, 2, 3, 4, 5, 6, 7, 8];
        let pixels = super::copy_readback_pixels(&mapped, 2, 1, 8);
        assert_eq!(pixels, mapped);
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

    #[tokio::test]
    async fn test_probe_invalid_uri() {
        let uri = "file:///non/existent/file.mp4".to_string();
        let result = probe_metadata(uri).await;
        assert!(result.is_err());
    }
}
