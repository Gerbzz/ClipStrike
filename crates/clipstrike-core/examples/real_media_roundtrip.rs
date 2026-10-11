//! Headless editorial smoke: real local asset links, clip edits, undo/redo, save/reopen.
//! This does not decode, play, composite, or export the linked media.
use clipstrike_core::editor::EditorSession;
use clipstrike_core::keyframes::KeyframedProperty;
use clipstrike_core::timeline::Clip;
use std::path::{Path, PathBuf};
use std::process::Command;

fn local_uri(path: &Path) -> Result<String, String> {
    if !path.is_file() || std::fs::metadata(path).map_err(|e| e.to_string())?.len() == 0 {
        return Err(format!("Missing or empty media: {}", path.display()));
    }
    url::Url::from_file_path(path)
        .map(|uri| uri.to_string())
        .map_err(|_| format!("Invalid media path: {}", path.display()))
}

fn probe_duration(path: &Path, needs_video: bool) -> Result<u64, String> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_type", "-show_entries", "format=duration", "-of", "json"])
        .arg(path)
        .output()
        .map_err(|error| format!("ffprobe could not start: {error}"))?;
    if !output.status.success() {
        return Err(format!("ffprobe rejected {}", path.display()));
    }
    let probe: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Invalid ffprobe output: {error}"))?;
    let streams = probe["streams"].as_array().ok_or("ffprobe returned no streams")?;
    let has_audio = streams.iter().any(|stream| stream["codec_type"] == "audio");
    let has_video = streams.iter().any(|stream| stream["codec_type"] == "video");
    if !has_audio || (needs_video && !has_video) {
        return Err(format!("Required decoded stream absent: {}", path.display()));
    }
    let seconds: f64 = probe["format"]["duration"].as_str()
        .ok_or("ffprobe returned no duration")?
        .parse().map_err(|_| "Invalid media duration")?;
    if !seconds.is_finite() || seconds < 2.0 {
        return Err(format!("Fixture must cover the two-second edit: {}", path.display()));
    }
    Ok((seconds * 1000.0).round() as u64)
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 4 {
        return Err("Usage: real_media_roundtrip <video-path> <audio-path> <project-path>".into());
    }
    let video = PathBuf::from(&args[1]);
    let audio = PathBuf::from(&args[2]);
    let project_path = PathBuf::from(&args[3]);
    let video_uri = local_uri(&video)?;
    let audio_uri = local_uri(&audio)?;
    let video_duration_ms = probe_duration(&video, true)?;
    let audio_duration_ms = probe_duration(&audio, false)?;

    let mut editor = EditorSession::with_default_project("Real media edit smoke", 1280, 720, 30.0, 2);
    let video_asset = editor.add_asset("Video with source sound", video_uri.clone(), video_duration_ms);
    let audio_asset = editor.add_asset("Separate WAV soundtrack", audio_uri.clone(), audio_duration_ms);
    let video_track = editor.project.tracks[0].id;
    let audio_track = editor.project.tracks[1].id;

    let first = Clip::new(1, video_asset, video_track, 0, 1000, 0, 1000);
    editor.add_clip(first);
    let mut second = Clip::new(2, video_asset, video_track, 1000, 2000, 1000, 2000);
    second.audio_enabled = false;
    editor.add_clip(second);
    let mut soundtrack = Clip::new(3, audio_asset, audio_track, 0, 2000, 0, 2000);
    soundtrack.video_enabled = false;
    soundtrack.volume = KeyframedProperty::new(0.35);
    editor.add_clip(soundtrack);

    editor.trim_clip(1, 100, 900)?;
    editor.undo();
    editor.redo();
    editor.move_clip(2, 950, 1950)?;
    editor.undo();
    editor.redo();
    let expected = editor.snapshot_project();
    editor.save_to_path(&project_path)?;
    let reopened = EditorSession::open_from_path(&project_path)?;
    if reopened.snapshot_project() != expected {
        return Err("Reopened edits differ from saved editorial state".into());
    }
    for (path, expected_uri) in [(&video, &video_uri), (&audio, &audio_uri)] {
        if local_uri(path)? != *expected_uri {
            return Err(format!("Media link changed: {}", path.display()));
        }
    }
    println!("project={} clips={} assets={} trim=100..900 move=950..1950 audio_gain=0.35 reopen=match",
        project_path.display(), reopened.timeline.clips.len(), reopened.project.assets.len());
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
