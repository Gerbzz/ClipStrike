use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipMetadata {
    pub duration: Option<Duration>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps_num: Option<i32>,
    pub fps_den: Option<i32>,
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

    #[tokio::test]
    async fn test_probe_invalid_uri() {
        let uri = "file:///non/existent/file.mp4".to_string();
        let result = probe_metadata(uri).await;
        assert!(result.is_err());
    }
}
