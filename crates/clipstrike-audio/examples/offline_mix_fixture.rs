//! Produces a deterministic two-source export-audio fixture for independent
//! inspection with ffprobe/ffmpeg. This is not a replacement for a real CS2 capture.

use clipstrike_audio::offline::{OfflineMix, PcmSource, write_f32_wav};
use clipstrike_core::keyframes::KeyframedProperty;
use clipstrike_core::project::{Project, Track};
use clipstrike_core::timeline::Clip;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn tone(rate: u32, frames: usize, hz: f64, amplitude: f32) -> Vec<f32> {
    let mut samples = Vec::with_capacity(frames * 2);
    for index in 0..frames {
        let value =
            (std::f64::consts::TAU * hz * index as f64 / rate as f64).sin() as f32 * amplitude;
        samples.extend_from_slice(&[value, value]);
    }
    samples
}

fn main() -> Result<(), String> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Pass a fixture output directory")?,
    );
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let first_path = directory.join("source-44100.wav");
    let second_path = directory.join("source-48000.wav");
    let output_path = directory.join("mixed-48000.wav");
    for path in [&first_path, &second_path, &output_path] {
        if path.exists() {
            return Err(format!(
                "Fixture destination already exists: {}",
                path.display()
            ));
        }
    }
    write_f32_wav(&first_path, 44_100, &tone(44_100, 66_150, 440.0, 0.2))?;
    write_f32_wav(&second_path, 48_000, &tone(48_000, 48_000, 660.0, 0.2))?;

    let mut project = Project::new("Offline mix fixture", 320, 180, 30.0);
    project.tracks = vec![Track::new(1, "Game"), Track::new(2, "Overlay")];
    let first = Clip::new(1, 11, 1, 0, 1500, 0, 1500);
    let mut second = Clip::new(2, 22, 2, 500, 1500, 0, 1000);
    second.volume = KeyframedProperty::new(0.5);
    let sources = HashMap::from([
        (11, Arc::new(PcmSource::read_wav(&first_path)?)),
        (22, Arc::new(PcmSource::read_wav(&second_path)?)),
    ]);
    let mix = OfflineMix::new(&project, &[first, second], &sources, 48_000, 60, 30, 1)?;
    mix.render_to_wav(&output_path)?;
    println!(
        "MIXED_PCM={} frames={} rate={} channels=2",
        output_path.display(),
        mix.total_frames(),
        mix.sample_rate()
    );
    Ok(())
}
