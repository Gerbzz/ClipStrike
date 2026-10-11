//! Exercises ClipStrike's audio decode, timeline mix, and soundtrack mux
//! independently of its GStreamer video renderer.

use clipstrike_core::keyframes::KeyframedProperty;
use clipstrike_core::project::{MediaAsset, Project, Track};
use clipstrike_core::timeline::Clip;
use clipstrike_media::export::ExportSettings;
use clipstrike_ui::audio_export::{finish_export, prepare_mix, verify_rendered_video};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let source = PathBuf::from(args.next().ok_or("Pass audio-bearing source MP4")?);
    let overlay = PathBuf::from(args.next().ok_or("Pass overlay WAV")?);
    let video = PathBuf::from(args.next().ok_or("Pass silent video MP4")?);
    let destination = PathBuf::from(args.next().ok_or("Pass new output MP4 path")?);
    if destination.exists() {
        return Err(format!("Destination exists: {}", destination.display()));
    }
    let uri = url::Url::from_file_path(&source)
        .map_err(|_| "Invalid source path")?
        .to_string();
    let overlay_uri = url::Url::from_file_path(&overlay)
        .map_err(|_| "Invalid overlay path")?
        .to_string();
    let mut project = Project::new("Audio mux fixture", 320, 180, 30.0);
    project.tracks = vec![
        Track::new(1, "Source audio"),
        Track::new(2, "Overlay audio"),
    ];
    project.assets.push(MediaAsset {
        id: 7,
        name: "AAC source".into(),
        uri,
        duration_ms: 2_000,
        audio_levels: Vec::new(),
    });
    project.assets.push(MediaAsset {
        id: 8,
        name: "WAV overlay".into(),
        uri: overlay_uri,
        duration_ms: 1_000,
        audio_levels: Vec::new(),
    });
    let mut overlay_clip = Clip::new(2, 8, 2, 500, 1_500, 0, 1_000);
    overlay_clip.volume = KeyframedProperty::new(0.5);
    let clips = [Clip::new(1, 7, 1, 0, 2_000, 0, 2_000), overlay_clip];
    let settings = ExportSettings {
        width: 320,
        height: 180,
        fps_num: 30,
        fps_den: 1,
        ..Default::default()
    };
    verify_rendered_video(&video, &settings, 60)?;
    if verify_rendered_video(&video, &settings, 61).is_ok() {
        return Err("Video frame-count gate accepted an incorrect frame count".into());
    }
    let mut silent_project = Project::new("Silent video check", 320, 180, 30.0);
    silent_project.tracks.push(Track::new(1, "Video"));
    silent_project.assets.push(MediaAsset {
        id: 99,
        name: "Silent video".into(),
        uri: url::Url::from_file_path(&video)
            .map_err(|_| "Invalid silent video path")?
            .to_string(),
        duration_ms: 2_000,
        audio_levels: Vec::new(),
    });
    let mut silent_clip = Clip::new(99, 99, 1, 0, 2_000, 0, 2_000);
    let absent_audio = prepare_mix(&silent_project, &[silent_clip.clone()], &settings, 60)
        .err()
        .ok_or("Enabled audio on silent video was accepted")?;
    if !absent_audio.contains("no audio stream") {
        return Err(absent_audio);
    }
    silent_clip.audio_enabled = false;
    if prepare_mix(&silent_project, &[silent_clip], &settings, 60)?.is_some() {
        return Err("Intentionally silent video unexpectedly produced a mix".into());
    }
    let mix =
        prepare_mix(&project, &clips, &settings, 60)?.ok_or("Source media had no audio stream")?;
    let staging = destination.with_file_name(format!(
        ".clipstrike-fixture-{}-{}.mp4",
        std::process::id(),
        rand::random::<u64>(),
    ));
    std::fs::copy(&video, &staging).map_err(|error| error.to_string())?;
    finish_export(
        &staging,
        &destination,
        &settings,
        Some(&mix),
        60,
        &AtomicBool::new(false),
    )?;
    println!(
        "MUXED_SOUNDTRACK={} frames={} rate={}",
        destination.display(),
        mix.total_frames(),
        mix.sample_rate()
    );
    Ok(())
}
