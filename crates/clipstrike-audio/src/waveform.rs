#[cfg(feature = "gstreamer-backend")]
pub fn extract_waveform_sync(uri: &str, bins: usize) -> Result<Vec<f32>, String> {
    use glib::prelude::Cast;
    use gstreamer as gst;
    use gstreamer::prelude::*;
    use gstreamer_app as gst_app;

    if bins == 0 {
        return Ok(Vec::new());
    }
    if let Err(err) = gst::init() {
        return Err(format!("Failed to initialize GStreamer: {}", err));
    }

    let pipeline = gst::parse::launch(&format!(
        "uridecodebin uri={:?} ! audioconvert ! audioresample ! audio/x-raw,format=F32LE,channels=1,rate=8000 ! appsink name=sink sync=false max-buffers=32 drop=true",
        uri
    ))
    .map_err(|err| clipstrike_core::media_uri::media_error(uri, "Failed to build waveform pipeline", &err.to_string()))?
    .dynamic_cast::<gst::Pipeline>()
    .map_err(|_| "Waveform pipeline was not a GstPipeline".to_string())?;
    let sink = pipeline
        .by_name("sink")
        .ok_or_else(|| "Waveform appsink was not created".to_string())?
        .dynamic_cast::<gst_app::AppSink>()
        .map_err(|_| "Waveform sink was not an AppSink".to_string())?;

    pipeline
        .set_state(gst::State::Playing)
        .map_err(|err| clipstrike_core::media_uri::media_error(uri, "Failed to start waveform pipeline", &err.to_string()))?;

    let bus = pipeline
        .bus()
        .ok_or_else(|| "Waveform pipeline has no bus".to_string())?;

    let mut samples = Vec::new();
    let mut consecutive_timeouts = 0u32;

    loop {
        if let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_mseconds(20)) {
            consecutive_timeouts = 0;
            let Some(buffer) = sample.buffer() else {
                continue;
            };
            let map = buffer
                .map_readable()
                .map_err(|_| "Failed to read waveform audio buffer".to_string())?;
            for chunk in map.as_slice().chunks_exact(4) {
                samples.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]).abs());
            }
        } else {
            consecutive_timeouts = consecutive_timeouts.saturating_add(1);
        }

        if let Some(message) = bus.timed_pop_filtered(
            gst::ClockTime::from_mseconds(5),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        ) {
            match message.view() {
                gst::MessageView::Eos(..) => break,
                gst::MessageView::Error(err) => {
                    let _ = pipeline.set_state(gst::State::Null);
                    return Err(clipstrike_core::media_uri::media_error(
                        uri, "Waveform decode failed", &err.error().to_string(),
                    ));
                }
                _ => {}
            }
        }

        if consecutive_timeouts > 250 {
            break;
        }
    }

    let _ = pipeline.set_state(gst::State::Null);
    if samples.is_empty() {
        return Ok(Vec::new());
    }

    let mut peaks = vec![0.0f32; bins];
    let total_samples = samples.len();
    for (idx, sample) in samples.into_iter().enumerate() {
        let bin = ((idx * bins) / total_samples).min(bins.saturating_sub(1));
        peaks[bin] = peaks[bin].max(sample);
    }

    let max_peak = peaks.iter().copied().fold(0.0f32, f32::max).max(1e-6);
    for peak in &mut peaks {
        *peak = (*peak / max_peak).clamp(0.0, 1.0);
    }
    Ok(peaks)
}

#[cfg(not(feature = "gstreamer-backend"))]
pub fn extract_waveform_sync(_uri: &str, _bins: usize) -> Result<Vec<f32>, String> {
    Ok(Vec::new())
}

pub async fn extract_waveform_async(uri: String, bins: usize) -> Result<Vec<f32>, String> {
    tokio::task::spawn_blocking(move || extract_waveform_sync(&uri, bins))
        .await
        .map_err(|e: tokio::task::JoinError| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_waveform_sync_invalid_uri() {
        let result = extract_waveform_sync("file:///non/existent/file.mp4", 64);
        #[cfg(feature = "gstreamer-backend")]
        assert!(result.is_err());
        #[cfg(not(feature = "gstreamer-backend"))]
        assert_eq!(result.unwrap(), Vec::<f32>::new());
    }

    #[test]
    fn test_waveform_sync_zero_bins() {
        let result = extract_waveform_sync("file:///non/existent/file.mp4", 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Vec::<f32>::new());
    }
}
