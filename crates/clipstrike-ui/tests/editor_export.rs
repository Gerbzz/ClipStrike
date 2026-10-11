#![cfg(all(not(target_arch = "wasm32"), feature = "gstreamer-backend"))]

use clipstrike_core::commands::{RazorClipCommand, UndoStack};
use clipstrike_core::keyframes::KeyframedProperty;
use clipstrike_core::project::{MediaAsset, Project, TitleAsset, TitleTemplate, Track};
use clipstrike_core::timeline::Clip;
use clipstrike_media::export::ExportSettings;
use clipstrike_ui::ui::ClipStrikeApp;
use std::path::{Path, PathBuf};
use std::process::Command;

fn file_uri(path: &Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

fn write_tone_wav(path: &Path) {
    let frames = 48_000_u32;
    let channels = 2_u16;
    let bits = 16_u16;
    let payload_bytes = frames * channels as u32 * (bits as u32 / 8);
    let mut bytes = Vec::with_capacity((44 + payload_bytes) as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + payload_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&frames.to_le_bytes());
    bytes.extend_from_slice(&(frames * channels as u32 * (bits as u32 / 8)).to_le_bytes());
    bytes.extend_from_slice(&(channels * (bits / 8)).to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&payload_bytes.to_le_bytes());
    for index in 0..frames {
        let sample = ((2.0 * std::f32::consts::PI * 660.0 * index as f32 / frames as f32).sin()
            * 12_000.0) as i16;
        for _ in 0..channels {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
    }
    std::fs::write(path, bytes).unwrap();
}

fn decoded_frame(source: &Path, time: &str, destination: &Path) -> image::RgbImage {
    let decode = Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-ss", time, "-i"])
        .arg(source)
        .args(["-frames:v", "1"])
        .arg(destination)
        .output()
        .unwrap();
    assert!(
        decode.status.success(),
        "{}",
        String::from_utf8_lossy(&decode.stderr)
    );
    image::open(destination).unwrap().to_rgb8()
}

fn mean_absolute_rgb_error(left: &image::RgbImage, right: &image::RgbImage) -> f64 {
    assert_eq!(left.dimensions(), right.dimensions());
    left.as_raw()
        .iter()
        .zip(right.as_raw())
        .map(|(a, b)| a.abs_diff(*b) as u64)
        .sum::<u64>() as f64
        / left.as_raw().len() as f64
}

#[test]
fn saved_trimmed_project_with_title_and_soundtrack_exports_through_editor_renderer() {
    let artifact_dir = std::env::var_os("CLIPSTRIKE_EDITOR_EXPORT_ARTIFACT_DIR").map(PathBuf::from);
    let dir = if let Some(root) = &artifact_dir {
        std::fs::create_dir_all(root).unwrap();
        tempfile::Builder::new()
            .prefix("reopenable-run-")
            .tempdir_in(root)
            .unwrap()
    } else {
        tempfile::tempdir().unwrap()
    };
    let video = dir.path().join("source.mp4");
    let soundtrack = dir.path().join("soundtrack.wav");
    let title_file = dir.path().join("title_42.svg");
    let project_file = dir.path().join("edited.cstrike");
    let output = dir.path().join("edited.mp4");
    std::fs::write(
        &video,
        include_bytes!("../../clipstrike-media/tests/fixtures/preview-with-audio.mp4"),
    )
    .unwrap();
    write_tone_wav(&soundtrack);
    let svg = "<svg xmlns='http://www.w3.org/2000/svg' width='320' height='180'><rect x='96' y='64' width='128' height='52' fill='#ed1c24'/><text x='160' y='98' text-anchor='middle' fill='white' font-size='24'>EDIT</text></svg>";
    std::fs::write(&title_file, svg).unwrap();

    let mut project = Project::new("Offscreen export", 320, 180, 10.0);
    project.tracks = vec![
        Track::new(1, "Video"),
        Track::new(2, "Title"),
        Track::new(3, "Soundtrack"),
    ];
    project.assets.push(MediaAsset {
        id: 1,
        name: "Source with sound".into(),
        uri: file_uri(&video),
        duration_ms: 2_000,
        audio_levels: vec![],
    });
    project.assets.push(MediaAsset {
        id: 2,
        name: "Title".into(),
        uri: file_uri(&title_file),
        duration_ms: 1_000,
        audio_levels: vec![],
    });
    project.assets.push(MediaAsset {
        id: 3,
        name: "Tone".into(),
        uri: file_uri(&soundtrack),
        duration_ms: 1_000,
        audio_levels: vec![],
    });
    project.titles.push(TitleAsset {
        id: 42,
        name: "Title".into(),
        template: TitleTemplate::Centered,
        text: "EDIT".into(),
        font_family: "Arial".into(),
        font_size: 24.0,
        color: [255, 255, 255, 255],
        x: 0.5,
        y: 0.5,
        svg_content: svg.into(),
    });
    let first = Clip::new(11, 1, 1, 0, 500, 100, 600);
    let second = Clip::new(12, 1, 1, 500, 1_000, 1_100, 1_600);
    let mut title = Clip::new(13, 2, 2, 300, 800, 0, 500);
    title.audio_enabled = false;
    let mut tone = Clip::new(14, 3, 3, 0, 1_000, 0, 1_000);
    tone.video_enabled = false;
    tone.volume = KeyframedProperty::new(0.4);
    project.clips = vec![first, second, title, tone];
    let mut timeline = project.to_timeline();
    let mut edits = UndoStack::new();
    edits.push(
        Box::new(RazorClipCommand {
            clip_id: 11,
            split_at: 250,
            original_clip: None,
            new_clip_id: None,
            split_clips: None,
        }),
        &mut project,
        &mut timeline,
    );
    assert_eq!(
        timeline.clips.len(),
        5,
        "razor must create one additional source segment"
    );
    project.capture_timeline(&timeline);
    project.save(&project_file).unwrap();
    let reopened = Project::load(&project_file).unwrap();
    assert_eq!(reopened.clips.len(), 5);
    let split_left = reopened.clips.iter().find(|clip| clip.id == 11).unwrap();
    assert_eq!(
        (
            split_left.start,
            split_left.end,
            split_left.in_point,
            split_left.out_point
        ),
        (0, 250, 100, 350)
    );
    let split_right = reopened
        .clips
        .iter()
        .find(|clip| clip.asset_id == 1 && clip.start == 250)
        .unwrap();
    assert_eq!(
        (split_right.end, split_right.in_point, split_right.out_point),
        (500, 350, 600)
    );
    let original_source = std::fs::read(&video).unwrap();
    let original_soundtrack = std::fs::read(&soundtrack).unwrap();
    let original_project = std::fs::read(&project_file).unwrap();

    let settings = ExportSettings {
        width: 320,
        height: 180,
        fps_num: 10,
        fps_den: 1,
        // Keep the repeatable proof independent of NVENC session availability.
        use_hardware_acceleration: false,
        ..Default::default()
    };
    ClipStrikeApp::export_saved_project_offscreen(&project_file, &output, settings.clone())
        .unwrap();
    assert!(output.metadata().unwrap().len() > 0);

    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            "stream=codec_type,nb_read_frames,duration,width,height",
            "-of",
            "json",
        ])
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        probe.status.success(),
        "{}",
        String::from_utf8_lossy(&probe.stderr)
    );
    let streams: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
    let streams = streams["streams"].as_array().unwrap();
    let video_stream = streams
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .unwrap();
    assert_eq!(video_stream["nb_read_frames"], "10");
    assert_eq!(video_stream["width"], 320);
    assert_eq!(video_stream["height"], 180);
    assert!(
        (video_stream["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            - 1.0)
            .abs()
            < 0.02
    );
    let audio_stream = streams
        .iter()
        .find(|stream| stream["codec_type"] == "audio")
        .unwrap();
    assert!(
        (audio_stream["duration"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            - 1.0)
            .abs()
            < 0.02
    );

    let decode = Command::new("ffmpeg")
        .args(["-v", "error", "-xerror", "-i"])
        .arg(&output)
        .args(["-f", "null", "NUL"])
        .output()
        .unwrap();
    assert!(
        decode.status.success(),
        "{}",
        String::from_utf8_lossy(&decode.stderr)
    );

    let out_early = decoded_frame(&output, "0.1", &dir.path().join("out-early.png"));
    let source_early = decoded_frame(&video, "0.2", &dir.path().join("source-early.png"));
    let out_late = decoded_frame(&output, "0.9", &dir.path().join("out-late.png"));
    let source_late = decoded_frame(&video, "1.5", &dir.path().join("source-late.png"));
    assert!(
        mean_absolute_rgb_error(&out_early, &source_early) < 12.0,
        "first trimmed segment differs from its decoded source frame"
    );
    assert!(
        mean_absolute_rgb_error(&out_late, &source_late) < 12.0,
        "reordered segment differs from its decoded source frame"
    );
    let with_title = decoded_frame(&output, "0.4", &dir.path().join("out-title.png"));
    let [red, green, blue] = with_title.get_pixel(110, 70).0;
    assert!(
        red > 150 && green < 80 && blue < 90,
        "title rectangle missing: {red},{green},{blue}"
    );
    assert!(
        out_late.get_pixel(110, 70).0[1] > 150,
        "title remains after its out-point"
    );

    let pcm = Command::new("ffmpeg")
        .args(["-v", "error", "-xerror", "-i"])
        .arg(&output)
        .args([
            "-map", "0:a:0", "-ac", "2", "-ar", "48000", "-f", "f32le", "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(
        pcm.status.success(),
        "{}",
        String::from_utf8_lossy(&pcm.stderr)
    );
    let left: Vec<f32> = pcm
        .stdout
        .chunks_exact(8)
        .map(|sample| f32::from_le_bytes(sample[0..4].try_into().unwrap()))
        .collect();
    assert!(left.len() >= 48_000);
    let window = &left[12_000..36_000];
    let tone_amplitude = |frequency: f64| -> f64 {
        let (sin, cos) =
            window
                .iter()
                .enumerate()
                .fold((0.0, 0.0), |(sin, cos), (index, sample)| {
                    let phase = std::f64::consts::TAU * frequency * index as f64 / 48_000.0;
                    (
                        sin + *sample as f64 * phase.sin(),
                        cos + *sample as f64 * phase.cos(),
                    )
                });
        2.0 * sin.hypot(cos) / window.len() as f64
    };
    assert!(tone_amplitude(660.0) > 0.08);
    assert!(
        tone_amplitude(440.0) > 0.04,
        "trimmed video source audio is missing from the mix"
    );

    assert_eq!(std::fs::read(&video).unwrap(), original_source);
    assert_eq!(std::fs::read(&soundtrack).unwrap(), original_soundtrack);
    assert_eq!(std::fs::read(&project_file).unwrap(), original_project);

    if let Some(artifact_dir) = artifact_dir {
        let retained_run = dir.keep();
        let copied_project = artifact_dir.join("edited.cstrike");
        std::fs::copy(&project_file, &copied_project).unwrap();
        std::fs::copy(&video, artifact_dir.join("source.mp4")).unwrap();
        std::fs::copy(&title_file, artifact_dir.join("title_42.svg")).unwrap();
        std::fs::copy(&soundtrack, artifact_dir.join("soundtrack.wav")).unwrap();
        std::fs::copy(&output, artifact_dir.join("edited.mp4")).unwrap();
        std::fs::write(
            artifact_dir.join("retained-run-path.txt"),
            retained_run.display().to_string(),
        )
        .unwrap();
        let retained_project = Project::load(&copied_project).unwrap();
        for asset in &retained_project.assets {
            let linked = url::Url::parse(&asset.uri).unwrap().to_file_path().unwrap();
            assert!(
                linked.is_file(),
                "retained project has a missing media asset: {}",
                linked.display()
            );
        }
        let reopened_output = retained_run.join("reopened-from-copied-project.mp4");
        ClipStrikeApp::export_saved_project_offscreen(&copied_project, &reopened_output, settings)
            .unwrap();
        let reopened_decode = Command::new("ffmpeg")
            .args(["-v", "error", "-xerror", "-i"])
            .arg(&reopened_output)
            .args(["-f", "null", "NUL"])
            .output()
            .unwrap();
        assert!(
            reopened_decode.status.success(),
            "{}",
            String::from_utf8_lossy(&reopened_decode.stderr)
        );
    }
}
