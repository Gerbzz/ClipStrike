//! Offline soundtrack preparation and muxing. All tools are invoked with
//! structured arguments; inputs are local files and originals are read only.

use clipstrike_audio::decode_f32le_samples;
use clipstrike_audio::offline::{OfflineMix, PcmSource};
use clipstrike_core::project::{MediaAsset, Project};
use clipstrike_core::timeline::Clip;
use clipstrike_media::export::{ExportFormat, ExportSettings};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use url::Url;

struct TemporaryFile(PathBuf);

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn local_asset_path(asset: &MediaAsset) -> Result<PathBuf, String> {
    if let Ok(url) = Url::parse(&asset.uri) {
        if url.scheme() != "file" {
            return Err(format!(
                "Only local audio sources can be exported: {}",
                asset.name
            ));
        }
        url.to_file_path()
            .map_err(|_| format!("Invalid local media URI: {}", asset.name))
    } else {
        Ok(PathBuf::from(&asset.uri))
    }
}

fn is_still_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "bmp" | "gif" | "webp" | "tiff" | "svg")
    )
}

fn checked_tool_output_with(
    command: &mut Command,
    tool: &str,
    cancel: Option<&AtomicBool>,
    timeout: Duration,
    capture_stdout: bool,
) -> Result<std::process::Output, String> {
    if capture_stdout {
        command.stdout(Stdio::piped());
    }
    command.stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("{tool} is required for export: {error}"))?;
    let started = Instant::now();
    loop {
        if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{tool} canceled"));
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{tool} timed out after {} seconds",
                timeout.as_secs_f64()
            ));
        }
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Could not wait for {tool}: {error}"));
            }
        };
        if status.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("Could not collect {tool} output: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{tool} failed: {}", detail.trim()));
    }
    Ok(output)
}

fn decode_source(path: &Path, through_ms: i64) -> Result<Option<PcmSource>, String> {
    decode_source_with_cancel(path, through_ms, None)
}

fn decode_source_with_cancel(
    path: &Path,
    through_ms: i64,
    cancel: Option<&AtomicBool>,
) -> Result<Option<PcmSource>, String> {
    if !path.is_file() {
        return Err(format!("Audio source is offline: {}", path.display()));
    }
    if is_still_image(path) {
        return Ok(None);
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("wav"))
    {
        return PcmSource::read_wav(path).map(Some);
    }
    let probe = checked_tool_output_with(
        Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-select_streams",
                "a:0",
                "-show_entries",
                "stream=index",
                "-of",
                "csv=p=0",
            ])
            .arg(path),
        "ffprobe",
        cancel,
        Duration::from_secs(60),
        true,
    )?;
    if probe.stdout.iter().all(|byte| byte.is_ascii_whitespace()) {
        return Ok(None);
    }

    let raw_path = std::env::temp_dir().join(format!(
        "clipstrike-audio-decode-{}-{}.f32le",
        std::process::id(),
        rand::random::<u64>(),
    ));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&raw_path)
        .map_err(|error| format!("Could not reserve audio decode file: {error}"))?;
    let raw = TemporaryFile(raw_path.clone());
    let seconds = format!("{:.3}", (through_ms.max(0) as f64 + 100.0) / 1000.0);
    checked_tool_output_with(
        Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-nostdin", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:a:0",
                "-vn",
                "-t",
                &seconds,
                "-ac",
                "2",
                "-ar",
                "48000",
                "-c:a",
                "pcm_f32le",
                "-f",
                "f32le",
                "-fs",
                "536870912",
                "pipe:1",
            ])
            .stdout(Stdio::from(file)),
        "ffmpeg",
        cancel,
        Duration::from_secs(600),
        false,
    )?;
    if std::fs::metadata(&raw.0)
        .map_err(|error| error.to_string())?
        .len()
        > 512 * 1024 * 1024
    {
        return Err("Decoded audio exceeds the current 512 MiB source limit".into());
    }
    let bytes = std::fs::read(&raw.0).map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        return Err(format!(
            "Audio decoder returned no samples: {}",
            path.display()
        ));
    }
    PcmSource::new(48_000, 2, decode_f32le_samples(&bytes)?).map(Some)
}

pub fn prepare_mix(
    project: &Project,
    timeline_clips: &[Clip],
    settings: &ExportSettings,
    video_frames: u64,
) -> Result<Option<OfflineMix>, String> {
    prepare_mix_with_decoder(
        project,
        timeline_clips,
        settings,
        video_frames,
        decode_source,
    )
}

pub fn prepare_mix_cancellable(
    project: &Project,
    timeline_clips: &[Clip],
    settings: &ExportSettings,
    video_frames: u64,
    cancel: &AtomicBool,
) -> Result<Option<OfflineMix>, String> {
    prepare_mix_with_decoder(
        project,
        timeline_clips,
        settings,
        video_frames,
        |path, through_ms| {
            if cancel.load(Ordering::Relaxed) {
                return Err("Export canceled".into());
            }
            decode_source_with_cancel(path, through_ms, Some(cancel))
        },
    )
}

fn prepare_mix_with_decoder(
    project: &Project,
    timeline_clips: &[Clip],
    settings: &ExportSettings,
    video_frames: u64,
    mut decoder: impl FnMut(&Path, i64) -> Result<Option<PcmSource>, String>,
) -> Result<Option<OfflineMix>, String> {
    let mut limits = BTreeMap::<usize, i64>::new();
    for clip in timeline_clips {
        if !clip.audio_enabled || !project.is_track_audible(clip.track_id) {
            continue;
        }
        if !project.assets.iter().any(|asset| asset.id == clip.asset_id) {
            return Err(format!(
                "Audio-enabled clip {} has no source asset",
                clip.id
            ));
        }
        limits
            .entry(clip.asset_id)
            .and_modify(|end| *end = (*end).max(clip.out_point))
            .or_insert(clip.out_point);
    }
    let mut sources = HashMap::<usize, Arc<PcmSource>>::new();
    for (asset_id, through_ms) in limits {
        let asset = project
            .assets
            .iter()
            .find(|asset| asset.id == asset_id)
            .unwrap();
        let path = local_asset_path(asset)?;
        if is_still_image(&path) {
            continue;
        }
        let decoded = decoder(&path, through_ms)?.ok_or_else(|| {
            format!(
                "Audio is enabled for '{}' but its source has no audio stream; disable audio on the clip for an intentional silent export",
                asset.name
            )
        })?;
        sources.insert(asset_id, Arc::new(decoded));
    }
    let audible_clips = timeline_clips
        .iter()
        .filter(|clip| sources.contains_key(&clip.asset_id))
        .cloned()
        .collect::<Vec<_>>();
    if audible_clips.is_empty() {
        return Ok(None);
    }
    let mix = OfflineMix::new(
        project,
        &audible_clips,
        &sources,
        48_000,
        video_frames,
        settings.fps_num,
        settings.fps_den,
    )?;
    Ok(Some(mix))
}

/// Produces a new mux staging file. The caller owns and publishes the result.
pub fn mux_soundtrack(
    video_path: &Path,
    destination: &Path,
    settings: &ExportSettings,
    mix: &OfflineMix,
    video_frames: u64,
) -> Result<PathBuf, String> {
    mux_soundtrack_inner(video_path, destination, settings, mix, video_frames, None)
}

fn mux_soundtrack_inner(
    video_path: &Path,
    destination: &Path,
    settings: &ExportSettings,
    mix: &OfflineMix,
    video_frames: u64,
    cancel: Option<&AtomicBool>,
) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or("Export destination has no parent directory")?;
    let unique = format!("{}-{}", std::process::id(), rand::random::<u64>());
    let audio_path = parent.join(format!(".clipstrike-audio-{unique}.wav"));
    let mux_path = parent.join(format!(".clipstrike-mux-{unique}.partial"));
    if let Some(cancel) = cancel {
        mix.render_to_wav_cancellable(&audio_path, cancel)?;
    } else {
        mix.render_to_wav(&audio_path)?;
    }
    let audio_path = TemporaryFile(audio_path);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&mux_path)
        .map_err(|error| format!("Could not reserve mux staging file: {error}"))?;
    let (format, codec) = match settings.format {
        ExportFormat::MP4 => ("mp4", "aac"),
        ExportFormat::MOV => ("mov", "aac"),
        ExportFormat::WebM => ("webm", "libopus"),
        ExportFormat::MKV => ("matroska", "aac"),
    };
    let result = checked_tool_output_with(
        Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-nostdin", "-y", "-i"])
            .arg(video_path)
            .arg("-i")
            .arg(&audio_path.0)
            .args([
                "-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", codec, "-b:a", "192k",
                "-f", format,
            ])
            .arg(&mux_path),
        "ffmpeg",
        cancel,
        Duration::from_secs(600),
        true,
    );
    if let Err(error) = result {
        let _ = std::fs::remove_file(&mux_path);
        return Err(error);
    }
    let probe = checked_tool_output_with(
        Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type",
                "-of",
                "csv=p=0",
            ])
            .arg(&mux_path),
        "ffprobe",
        cancel,
        Duration::from_secs(60),
        true,
    );
    let probe = match probe {
        Ok(probe) => probe,
        Err(error) => {
            let _ = std::fs::remove_file(&mux_path);
            return Err(error);
        }
    };
    let streams = String::from_utf8_lossy(&probe.stdout);
    if !streams.lines().any(|line| line.trim() == "video")
        || !streams.lines().any(|line| line.trim() == "audio")
    {
        let _ = std::fs::remove_file(&mux_path);
        return Err("Muxed output is missing video or audio".into());
    }
    let duration = checked_tool_output_with(
        Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(&mux_path),
        "ffprobe",
        cancel,
        Duration::from_secs(60),
        true,
    );
    let duration = duration.and_then(|output| {
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<f64>()
            .map_err(|error| format!("Could not read muxed duration: {error}"))
    });
    let expected_seconds = mix.total_frames() as f64 / mix.sample_rate() as f64;
    if !duration.is_ok_and(|seconds| (seconds - expected_seconds).abs() <= 0.1) {
        let _ = std::fs::remove_file(&mux_path);
        return Err("Muxed output duration differs from the audio/video render contract".into());
    }
    if let Err(error) = verify_rendered_video_inner(&mux_path, settings, video_frames, cancel) {
        let _ = std::fs::remove_file(&mux_path);
        return Err(error);
    }
    Ok(mux_path)
}

/// Checks the published video stream and fully decodes the container before success.
pub fn verify_rendered_video(
    path: &Path,
    settings: &ExportSettings,
    expected_frames: u64,
) -> Result<(), String> {
    verify_rendered_video_inner(path, settings, expected_frames, None)
}

fn verify_rendered_video_inner(
    path: &Path,
    settings: &ExportSettings,
    expected_frames: u64,
    cancel: Option<&AtomicBool>,
) -> Result<(), String> {
    if expected_frames == 0 || settings.fps_num <= 0 || settings.fps_den <= 0 {
        return Err("Invalid expected export frame count or FPS".into());
    }
    let probe = checked_tool_output_with(
        Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-count_frames",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=nb_read_frames",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path),
        "ffprobe",
        cancel,
        Duration::from_secs(600),
        true,
    )?;
    let decoded_frames = String::from_utf8_lossy(&probe.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|error| format!("Could not count exported video frames: {error}"))?;
    if decoded_frames != expected_frames {
        return Err(format!(
            "Exported video has {decoded_frames} frames; expected {expected_frames}"
        ));
    }
    let duration = checked_tool_output_with(
        Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path),
        "ffprobe",
        cancel,
        Duration::from_secs(60),
        true,
    )?;
    let actual_seconds = String::from_utf8_lossy(&duration.stdout)
        .trim()
        .parse::<f64>()
        .map_err(|error| format!("Could not read exported video duration: {error}"))?;
    let expected_seconds =
        expected_frames as f64 * settings.fps_den as f64 / settings.fps_num as f64;
    if (actual_seconds - expected_seconds).abs() > 0.1 {
        return Err(format!(
            "Exported video duration is {actual_seconds:.3} s; expected {expected_seconds:.3} s"
        ));
    }
    checked_tool_output_with(
        Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-xerror", "-nostdin", "-i"])
            .arg(path)
            .args(["-f", "null", "-"]),
        "ffmpeg",
        cancel,
        Duration::from_secs(600),
        true,
    )
    .map(|_| ())
    .map_err(|error| format!("Exported media did not fully decode: {error}"))
}

/// Owns verified staging files until the UI thread performs the final publish check.
pub struct PreparedExport {
    publish_file: TemporaryFile,
    _source_video: Option<TemporaryFile>,
}

impl PreparedExport {
    pub fn publish_if(
        self,
        destination: &Path,
        may_publish: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        may_publish()?;
        if destination.exists() {
            return Err(format!(
                "Export destination already exists: {}",
                destination.display()
            ));
        }
        // Staging is a sibling of the destination. A hard link atomically
        // refuses an existing name; Drop then removes only our staging names.
        std::fs::hard_link(&self.publish_file.0, destination).map_err(|error| {
            format!("Could not publish export without replacing an existing file: {error}")
        })?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn fixture_from_staging(path: PathBuf) -> Self {
        Self {
            publish_file: TemporaryFile(path),
            _source_video: None,
        }
    }
}

/// Decode and verify on a worker; publication remains on the UI thread.
pub fn prepare_final_export(
    staging_path: &Path,
    destination: &Path,
    settings: &ExportSettings,
    mix: Option<&OfflineMix>,
    video_frames: u64,
    cancel: &AtomicBool,
) -> Result<PreparedExport, String> {
    let staging = TemporaryFile(staging_path.to_path_buf());
    if cancel.load(Ordering::Relaxed) {
        return Err("Export canceled".into());
    }
    let size = std::fs::metadata(&staging.0)
        .map_err(|error| format!("Export output is missing: {error}"))?
        .len();
    if size == 0 {
        return Err("Export output is empty".into());
    }
    if destination.exists() {
        return Err(format!(
            "Export destination already exists: {}",
            destination.display()
        ));
    }
    if let Some(mix) = mix {
        let mux = mux_soundtrack_inner(
            &staging.0,
            destination,
            settings,
            mix,
            video_frames,
            Some(cancel),
        )?;
        let mux = TemporaryFile(mux);
        if cancel.load(Ordering::Relaxed) {
            return Err("Export canceled".into());
        }
        Ok(PreparedExport {
            publish_file: mux,
            _source_video: Some(staging),
        })
    } else {
        verify_rendered_video_inner(&staging.0, settings, video_frames, Some(cancel))?;
        if cancel.load(Ordering::Relaxed) {
            return Err("Export canceled".into());
        }
        Ok(PreparedExport {
            publish_file: staging,
            _source_video: None,
        })
    }
}

/// Convenience path for headless fixtures; the editor uses prepare_final_export.
pub fn finish_export(
    staging_path: &Path,
    destination: &Path,
    settings: &ExportSettings,
    mix: Option<&OfflineMix>,
    video_frames: u64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    prepare_final_export(
        staging_path,
        destination,
        settings,
        mix,
        video_frames,
        cancel,
    )?
    .publish_if(destination, || {
        if cancel.load(Ordering::Relaxed) {
            Err("Export canceled".into())
        } else {
            Ok(())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipstrike_core::project::Track;

    #[cfg(windows)]
    #[test]
    fn hung_and_failing_subprocesses_report_errors_and_stop() {
        let started = Instant::now();
        let timeout = checked_tool_output_with(
            Command::new("powershell").args(["-NoProfile", "-Command", "Start-Sleep -Seconds 3"]),
            "fixture",
            None,
            Duration::from_millis(120),
            true,
        )
        .err()
        .unwrap();
        assert!(timeout.contains("timed out"), "{timeout}");
        assert!(started.elapsed() < Duration::from_secs(2));

        let failure = checked_tool_output_with(
            Command::new("powershell").args(["-NoProfile", "-Command", "exit 7"]),
            "fixture",
            None,
            Duration::from_secs(3),
            true,
        )
        .err()
        .unwrap();
        assert!(failure.contains("failed"), "{failure}");

        let canceled = AtomicBool::new(true);
        let error = checked_tool_output_with(
            Command::new("powershell").args(["-NoProfile", "-Command", "Start-Sleep -Seconds 3"]),
            "fixture",
            Some(&canceled),
            Duration::from_secs(3),
            true,
        )
        .err()
        .unwrap();
        assert!(error.contains("canceled"), "{error}");
    }

    #[test]
    fn missing_audio_stream_requires_explicit_silent_clip() {
        let mut project = Project::new("Silent source", 320, 180, 30.0);
        project.tracks.push(Track::new(1, "Game"));
        project.assets.push(MediaAsset {
            id: 7,
            name: "Silent game video".into(),
            uri: "silent.mp4".into(),
            duration_ms: 1000,
            audio_levels: Vec::new(),
        });
        let mut clip = Clip::new(1, 7, 1, 0, 1000, 0, 1000);
        let settings = ExportSettings::default();
        let error =
            prepare_mix_with_decoder(&project, &[clip.clone()], &settings, 30, |_, _| Ok(None))
                .err()
                .unwrap();
        assert!(error.contains("disable audio on the clip"), "{error}");

        clip.audio_enabled = false;
        assert!(
            prepare_mix_with_decoder(&project, &[clip], &settings, 30, |_, _| {
                panic!("Muted clip should not decode")
            })
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn still_image_does_not_require_an_audio_stream() {
        let mut project = Project::new("Still", 320, 180, 30.0);
        project.tracks.push(Track::new(1, "Title"));
        project.assets.push(MediaAsset {
            id: 7,
            name: "Title".into(),
            uri: "title.svg".into(),
            duration_ms: 1000,
            audio_levels: Vec::new(),
        });
        let clip = Clip::new(1, 7, 1, 0, 1000, 0, 1000);
        assert!(
            prepare_mix_with_decoder(&project, &[clip], &ExportSettings::default(), 30, |_, _| {
                panic!("Still image should not decode audio")
            })
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn canceled_finalization_removes_only_its_staging_file() {
        let root = std::env::temp_dir();
        let unique = format!("{}-{}", std::process::id(), rand::random::<u64>());
        let staging = root.join(format!("clipstrike-cancel-{unique}.partial"));
        let destination = root.join(format!("clipstrike-existing-{unique}.mp4"));
        std::fs::write(&staging, b"partial video").unwrap();
        std::fs::write(&destination, b"existing output").unwrap();
        let cancel = AtomicBool::new(true);
        let error = finish_export(
            &staging,
            &destination,
            &ExportSettings::default(),
            None,
            30,
            &cancel,
        )
        .err()
        .unwrap();
        assert!(error.contains("canceled"));
        assert!(!staging.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"existing output");
        std::fs::remove_file(destination).unwrap();
    }

    #[test]
    fn publication_refuses_existing_destination_and_cleans_stage() {
        let root = std::env::temp_dir();
        let unique = format!("{}-{}", std::process::id(), rand::random::<u64>());
        let staging = root.join(format!("clipstrike-publish-stage-{unique}.partial"));
        let destination = root.join(format!("clipstrike-publish-existing-{unique}.mp4"));
        std::fs::write(&staging, b"verified stage").unwrap();
        std::fs::write(&destination, b"user's movie").unwrap();
        let error = PreparedExport::fixture_from_staging(staging.clone())
            .publish_if(&destination, || Ok(()))
            .err()
            .unwrap();
        assert!(error.contains("already exists"), "{error}");
        assert!(!staging.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"user's movie");
        std::fs::remove_file(destination).unwrap();
    }
}
