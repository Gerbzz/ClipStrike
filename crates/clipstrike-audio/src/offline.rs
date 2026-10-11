//! Deterministic, chunked PCM mixing for export. Source decoding is separate
//! from this module; an export must supply every audible source or fail.

use clipstrike_core::keyframes::{InterpolationMode, Keyframe, KeyframedProperty};
use clipstrike_core::project::Project;
use clipstrike_core::timeline::Clip;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug)]
pub struct PcmSource {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl PcmSource {
    pub fn new(sample_rate: u32, channels: u16, samples: Vec<f32>) -> Result<Self, String> {
        if sample_rate == 0 || !matches!(channels, 1 | 2) {
            return Err(
                "Offline audio requires a positive sample rate and mono or stereo PCM".into(),
            );
        }
        if samples.len() % channels as usize != 0
            || samples.iter().any(|sample| !sample.is_finite())
        {
            return Err("Offline audio contains incomplete or non-finite PCM frames".into());
        }
        Ok(Self {
            sample_rate,
            channels,
            samples,
        })
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    pub fn read_wav(path: &Path) -> Result<Self, String> {
        const LIMIT: u64 = 512 * 1024 * 1024;
        let size = std::fs::metadata(path)
            .map_err(|error| format!("{}: {error}", path.display()))?
            .len();
        if size > LIMIT {
            return Err(format!(
                "WAV exceeds the current 512 MiB offline decode limit: {}",
                path.display()
            ));
        }
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err(format!("Not a RIFF/WAVE file: {}", path.display()));
        }
        let declared_end =
            8usize.saturating_add(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize);
        if declared_end > bytes.len() || declared_end < 12 {
            return Err("Truncated WAV RIFF payload".into());
        }
        let mut offset = 12;
        let mut format = None;
        let mut data = None;
        while offset + 8 <= declared_end {
            let length =
                u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let begin = offset + 8;
            let end = begin
                .checked_add(length)
                .ok_or("WAV chunk length overflow")?;
            if end > declared_end {
                return Err("Truncated WAV chunk".into());
            }
            match &bytes[offset..offset + 4] {
                b"fmt " => format = Some(&bytes[begin..end]),
                b"data" => data = Some(&bytes[begin..end]),
                _ => {}
            }
            offset = end
                .checked_add(length & 1)
                .ok_or("WAV chunk padding overflow")?;
        }
        let format = format.ok_or("WAV is missing fmt chunk")?;
        let data = data.ok_or("WAV is missing data chunk")?;
        if format.len() < 16 {
            return Err("WAV fmt chunk is too short".into());
        }
        let mut encoding = u16::from_le_bytes(format[0..2].try_into().unwrap());
        let channels = u16::from_le_bytes(format[2..4].try_into().unwrap());
        let sample_rate = u32::from_le_bytes(format[4..8].try_into().unwrap());
        let block_align = u16::from_le_bytes(format[12..14].try_into().unwrap()) as usize;
        let bits = u16::from_le_bytes(format[14..16].try_into().unwrap());
        if encoding == 0xfffe {
            if format.len() < 40 {
                return Err("WAV extensible fmt chunk is too short".into());
            }
            let guid = &format[24..40];
            if guid[2..] != [0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71] {
                return Err("Unsupported WAV extensible subtype".into());
            }
            encoding = u16::from_le_bytes(guid[0..2].try_into().unwrap());
        }
        let bytes_per_sample = (bits as usize).div_ceil(8);
        if !matches!(channels, 1 | 2)
            || sample_rate == 0
            || block_align != bytes_per_sample * channels as usize
            || data.len() % block_align.max(1) != 0
        {
            return Err("Invalid WAV sample rate, channel count, or block alignment".into());
        }
        if !matches!((encoding, bits), (1, 16 | 24 | 32) | (3, 32)) {
            return Err(format!(
                "Unsupported WAV encoding {encoding} with {bits} bits"
            ));
        }
        let mut samples = Vec::with_capacity(data.len() / bytes_per_sample);
        for sample in data.chunks_exact(bytes_per_sample) {
            let value = match (encoding, bits) {
                (1, 16) => i16::from_le_bytes(sample.try_into().unwrap()) as f32 / 32768.0,
                (1, 24) => {
                    let signed = i32::from_le_bytes([
                        sample[0],
                        sample[1],
                        sample[2],
                        if sample[2] & 0x80 != 0 { 0xff } else { 0 },
                    ]);
                    signed as f32 / 8_388_608.0
                }
                (1, 32) => i32::from_le_bytes(sample.try_into().unwrap()) as f32 / 2_147_483_648.0,
                (3, 32) => f32::from_le_bytes(sample.try_into().unwrap()),
                _ => unreachable!(),
            };
            samples.push(value);
        }
        Self::new(sample_rate, channels, samples)
    }

    fn stereo_at(&self, index: usize) -> [f32; 2] {
        let start = index * self.channels as usize;
        if self.channels == 1 {
            [self.samples[start], self.samples[start]]
        } else {
            [self.samples[start], self.samples[start + 1]]
        }
    }
}

#[derive(Clone)]
struct GainEnvelope {
    default: f32,
    keys: Vec<Keyframe>,
}

impl GainEnvelope {
    fn new(property: &KeyframedProperty) -> Result<Self, String> {
        if !property.default_value.is_finite()
            || property.keyframes.iter().any(|key| !key.value.is_finite())
        {
            return Err("Non-finite audio gain is not exportable".into());
        }
        let mut keys = property.keyframes.clone();
        keys.sort_by_key(|key| key.time_ms);
        if keys
            .windows(2)
            .any(|pair| pair[0].time_ms == pair[1].time_ms)
        {
            return Err("Duplicate audio gain keyframe times are not exportable".into());
        }
        Ok(Self {
            default: property.default_value,
            keys,
        })
    }

    fn value_at(&self, time_ms: f64) -> f32 {
        let Some(first) = self.keys.first() else {
            return self.default;
        };
        if time_ms <= first.time_ms as f64 {
            return first.value;
        }
        let last = self.keys.last().unwrap();
        if time_ms >= last.time_ms as f64 {
            return last.value;
        }
        let next = self
            .keys
            .partition_point(|key| (key.time_ms as f64) < time_ms);
        let left = &self.keys[next - 1];
        let right = &self.keys[next];
        let progress =
            ((time_ms - left.time_ms as f64) / (right.time_ms - left.time_ms) as f64) as f32;
        match left.mode {
            InterpolationMode::Constant => left.value,
            InterpolationMode::Linear => left.value + (right.value - left.value) * progress,
            InterpolationMode::Bezier => {
                let control = left.value + (right.value - left.value) * 0.8;
                let inv = 1.0 - progress;
                inv * inv * left.value
                    + 2.0 * inv * progress * control
                    + progress * progress * right.value
            }
        }
    }
}

struct MixClip {
    start_ms: i64,
    end_ms: i64,
    in_point_ms: i64,
    source: Arc<PcmSource>,
    gain: GainEnvelope,
}

pub struct OfflineMix {
    clips: Vec<MixClip>,
    master_gain: f32,
    sample_rate: u32,
    total_frames: u64,
}

impl OfflineMix {
    pub fn new(
        project: &Project,
        timeline_clips: &[Clip],
        sources: &HashMap<usize, Arc<PcmSource>>,
        sample_rate: u32,
        video_frames: u64,
        fps_num: i32,
        fps_den: i32,
    ) -> Result<Self, String> {
        if sample_rate == 0 || fps_num <= 0 || fps_den <= 0 || video_frames == 0 {
            return Err("Invalid export sample rate, frame count, or FPS".into());
        }
        if !project.master_volume.is_finite() {
            return Err("Non-finite master volume is not exportable".into());
        }
        let total_frames = (video_frames as u128 * fps_den as u128 * sample_rate as u128)
            .div_ceil(fps_num as u128);
        if total_frames > u64::MAX as u128 {
            return Err("Audio export duration exceeds supported range".into());
        }
        let mut sorted = timeline_clips.iter().collect::<Vec<_>>();
        sorted.sort_by_key(|clip| (clip.start, clip.track_id, clip.id));
        let mut clips = Vec::new();
        for clip in sorted {
            if !clip.audio_enabled || !project.is_track_audible(clip.track_id) {
                continue;
            }
            if clip.start < 0
                || clip.end <= clip.start
                || clip.in_point < 0
                || clip.out_point <= clip.in_point
            {
                return Err(format!("Invalid audio clip timing for clip {}", clip.id));
            }
            if clip.reverse
                || clip.freeze_frame_ms.is_some()
                || clip.speed.default_value != 1.0
                || !clip.speed.keyframes.is_empty()
                || ((clip.end - clip.start) - (clip.out_point - clip.in_point)).abs() > 1
            {
                return Err(format!(
                    "Audio retiming is not yet exportable for clip {}",
                    clip.id
                ));
            }
            let source = sources.get(&clip.asset_id).ok_or_else(|| {
                format!(
                    "Decoded audio is missing for clip {} (asset {})",
                    clip.id, clip.asset_id
                )
            })?;
            // Out-points are exclusive. Every source frame before the out-point
            // must exist; never hide a truncated decode with trailing silence.
            let required_frames =
                (clip.out_point as u128 * source.sample_rate as u128).div_ceil(1000);
            if required_frames > source.frames() as u128 {
                return Err(format!(
                    "Audio source for clip {} ends at {} frames, but its out-point requires {} frames; shorten the trim or relink the full source",
                    clip.id,
                    source.frames(),
                    required_frames
                ));
            }
            clips.push(MixClip {
                start_ms: clip.start,
                end_ms: clip.end,
                in_point_ms: clip.in_point,
                source: Arc::clone(source),
                gain: GainEnvelope::new(&clip.volume)?,
            });
        }
        Ok(Self {
            clips,
            master_gain: project.master_volume,
            sample_rate,
            total_frames: total_frames as u64,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }
    pub fn has_audio(&self) -> bool {
        !self.clips.is_empty()
    }

    /// Returns interleaved stereo f32 samples for an exact output-frame range.
    pub fn render_chunk(&self, start_frame: u64, frame_count: usize) -> Result<Vec<f32>, String> {
        if start_frame
            .checked_add(frame_count as u64)
            .is_none_or(|end| end > self.total_frames)
        {
            return Err("Offline audio chunk exceeds export duration".into());
        }
        let mut output = Vec::with_capacity(frame_count * 2);
        let output_rate = self.sample_rate as i128;
        for offset in 0..frame_count {
            let frame = start_frame + offset as u64;
            let timeline_num = frame as i128 * 1000;
            let time_ms = timeline_num as f64 / self.sample_rate as f64;
            let mut mixed = [0.0f64; 2];
            for clip in &self.clips {
                if timeline_num < clip.start_ms as i128 * output_rate
                    || timeline_num >= clip.end_ms as i128 * output_rate
                {
                    continue;
                }
                let source_rate = clip.source.sample_rate as i128;
                let source_num = (timeline_num
                    + (clip.in_point_ms - clip.start_ms) as i128 * output_rate)
                    * source_rate;
                let denominator = 1000 * output_rate;
                if source_num < 0 {
                    continue;
                }
                let base = (source_num / denominator) as usize;
                if base >= clip.source.frames() {
                    return Err(format!(
                        "Audio source ended while rendering at {} ms; shorten or relink the clip",
                        time_ms
                    ));
                }
                let fraction = (source_num % denominator) as f32 / denominator as f32;
                let first = clip.source.stereo_at(base);
                let second = clip
                    .source
                    .stereo_at((base + 1).min(clip.source.frames() - 1));
                let gain = clip.gain.value_at(time_ms) as f64;
                for channel in 0..2 {
                    mixed[channel] +=
                        (first[channel] + (second[channel] - first[channel]) * fraction) as f64
                            * gain;
                }
            }
            for value in mixed {
                output.push((value * self.master_gain as f64).clamp(-1.0, 1.0) as f32);
            }
        }
        Ok(output)
    }

    /// Streams the exact export-duration stereo mix to an owned WAV staging file.
    pub fn render_to_wav(&self, path: &Path) -> Result<(), String> {
        self.render_to_wav_with_cancel(path, None)
    }

    pub fn render_to_wav_cancellable(
        &self,
        path: &Path,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<(), String> {
        self.render_to_wav_with_cancel(path, Some(cancel))
    }

    fn render_to_wav_with_cancel(
        &self,
        path: &Path,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<(), String> {
        use std::io::Write;
        let sample_count = self
            .total_frames
            .checked_mul(2)
            .ok_or("Audio export is too long")?;
        let data_bytes = sample_count
            .checked_mul(4)
            .ok_or("Audio export is too long")?;
        let data_bytes =
            u32::try_from(data_bytes).map_err(|_| "Audio export exceeds the WAV size limit")?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("Could not create audio staging file: {error}"))?;
        let mut writer = std::io::BufWriter::new(file);
        let result = (|| {
            write_wav_header(&mut writer, self.sample_rate, data_bytes)?;
            let mut cursor = 0;
            while cursor < self.total_frames {
                if cancel.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
                    return Err("Audio export canceled".into());
                }
                let count = (self.total_frames - cursor).min(4096) as usize;
                for sample in self.render_chunk(cursor, count)? {
                    writer
                        .write_all(&sample.to_le_bytes())
                        .map_err(|error| error.to_string())?;
                }
                cursor += count as u64;
            }
            writer.flush().map_err(|error| error.to_string())
        })();
        drop(writer);
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
    }
}

/// Write a small or streamed verification fixture as IEEE-float stereo WAV.
pub fn write_f32_wav(path: &Path, sample_rate: u32, samples: &[f32]) -> Result<(), String> {
    use std::io::Write;
    if sample_rate == 0
        || sample_rate > u32::MAX / 8
        || samples.len() % 2 != 0
        || samples.iter().any(|sample| !sample.is_finite())
    {
        return Err("Invalid stereo PCM for WAV output".into());
    }
    let data_bytes = samples
        .len()
        .checked_mul(4)
        .ok_or("WAV output is too large")?;
    let data_bytes = u32::try_from(data_bytes).map_err(|_| "WAV output is too large")?;
    let mut file =
        std::io::BufWriter::new(std::fs::File::create(path).map_err(|error| error.to_string())?);
    write_wav_header(&mut file, sample_rate, data_bytes)?;
    for sample in samples {
        file.write_all(&sample.to_le_bytes())
            .map_err(|error| error.to_string())?;
    }
    file.flush().map_err(|error| error.to_string())
}

fn write_wav_header(
    file: &mut impl std::io::Write,
    sample_rate: u32,
    data_bytes: u32,
) -> Result<(), String> {
    if sample_rate == 0 || sample_rate > u32::MAX / 8 {
        return Err("Invalid WAV sample rate".into());
    }
    let riff_size = data_bytes
        .checked_add(36)
        .ok_or("WAV output is too large")?;
    file.write_all(b"RIFF").map_err(|error| error.to_string())?;
    file.write_all(&riff_size.to_le_bytes())
        .map_err(|error| error.to_string())?;
    file.write_all(b"WAVEfmt ")
        .map_err(|error| error.to_string())?;
    file.write_all(&16u32.to_le_bytes())
        .map_err(|error| error.to_string())?;
    file.write_all(&3u16.to_le_bytes())
        .map_err(|error| error.to_string())?;
    file.write_all(&2u16.to_le_bytes())
        .map_err(|error| error.to_string())?;
    file.write_all(&sample_rate.to_le_bytes())
        .map_err(|error| error.to_string())?;
    file.write_all(&(sample_rate * 8).to_le_bytes())
        .map_err(|error| error.to_string())?;
    file.write_all(&8u16.to_le_bytes())
        .map_err(|error| error.to_string())?;
    file.write_all(&32u16.to_le_bytes())
        .map_err(|error| error.to_string())?;
    file.write_all(b"data").map_err(|error| error.to_string())?;
    file.write_all(&data_bytes.to_le_bytes())
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipstrike_core::project::Track;

    fn constant(rate: u32, channels: u16, frames: usize, values: [f32; 2]) -> Arc<PcmSource> {
        let mut samples = Vec::with_capacity(frames * channels as usize);
        for _ in 0..frames {
            samples.push(values[0]);
            if channels == 2 {
                samples.push(values[1]);
            }
        }
        Arc::new(PcmSource::new(rate, channels, samples).unwrap())
    }

    fn stereo(samples: &[f32], frame: usize) -> [f32; 2] {
        [samples[frame * 2], samples[frame * 2 + 1]]
    }

    #[test]
    fn placement_overlap_gain_and_solo_follow_timeline() {
        let mut project = Project::new("Mix", 320, 180, 30.0);
        project.tracks = vec![Track::new(1, "A"), Track::new(2, "B")];
        project.master_volume = 0.8;
        let mut first = Clip::new(1, 11, 1, 500, 1500, 250, 1250);
        first.volume = KeyframedProperty::new(0.5);
        let second = Clip::new(2, 22, 2, 250, 750, 0, 500);
        let sources = HashMap::from([
            (11, constant(44_100, 2, 88_200, [0.25, -0.25])),
            (22, constant(48_000, 1, 48_000, [0.5, 0.0])),
        ]);
        let clips = [first, second];
        let mix = OfflineMix::new(&project, &clips, &sources, 48_000, 30, 30, 1).unwrap();
        assert_eq!(mix.total_frames(), 48_000);
        assert_eq!(stereo(&mix.render_chunk(0, 1).unwrap(), 0), [0.0, 0.0]);
        let before = mix.render_chunk(24_000 - 1, 1).unwrap();
        assert_eq!(stereo(&before, 0), [0.4, 0.4]);
        let overlap = mix.render_chunk(24_000, 1).unwrap();
        assert!((overlap[0] - 0.5).abs() < 0.00001);
        assert!((overlap[1] - 0.3).abs() < 0.00001);
        assert_eq!(
            stereo(&mix.render_chunk(36_000, 1).unwrap(), 0),
            [0.1, -0.1]
        );

        project.tracks[1].solo = true;
        let solo = OfflineMix::new(&project, &clips, &sources, 48_000, 30, 30, 1).unwrap();
        assert_eq!(
            stereo(&solo.render_chunk(24_000, 1).unwrap(), 0),
            [0.4, 0.4]
        );
        project.tracks[1].solo = false;
        project.tracks[1].audio_enabled = false;
        let muted = OfflineMix::new(&project, &clips, &sources, 48_000, 30, 30, 1).unwrap();
        assert_eq!(
            stereo(&muted.render_chunk(24_000, 1).unwrap(), 0),
            [0.1, -0.1]
        );
    }

    #[test]
    fn trim_and_resampling_use_source_time_without_ms_stepping() {
        let mut project = Project::new("Trim", 320, 180, 30.0);
        project.tracks.push(Track::new(1, "A"));
        let clip = Clip::new(1, 11, 1, 250, 750, 500, 1000);
        let source = PcmSource::new(
            1_000,
            1,
            (0..1100).map(|frame| frame as f32 / 1000.0).collect(),
        )
        .unwrap();
        let sources = HashMap::from([(11, Arc::new(source))]);
        let mix = OfflineMix::new(&project, &[clip], &sources, 2_000, 30, 30, 1).unwrap();
        assert_eq!(stereo(&mix.render_chunk(500, 1).unwrap(), 0), [0.5, 0.5]);
        let half_source_frame = mix.render_chunk(501, 1).unwrap();
        assert!((half_source_frame[0] - 0.5005).abs() < 0.00001);
        assert_eq!(stereo(&mix.render_chunk(1500, 1).unwrap(), 0), [0.0, 0.0]);
    }

    #[test]
    fn exact_source_boundary_is_valid_but_truncated_decode_fails() {
        let mut project = Project::new("Boundary", 320, 180, 30.0);
        project.tracks.push(Track::new(1, "Audio"));
        let clip = Clip::new(1, 11, 1, 0, 1000, 0, 1000);
        let complete = HashMap::from([(11, constant(48_000, 1, 48_000, [0.25, 0.0]))]);
        let mix = OfflineMix::new(&project, &[clip.clone()], &complete, 48_000, 30, 30, 1).unwrap();
        assert_eq!(mix.render_chunk(47_999, 1).unwrap(), [0.25, 0.25]);

        let truncated = HashMap::from([(11, constant(48_000, 1, 47_999, [0.25, 0.0]))]);
        let error = OfflineMix::new(&project, &[clip], &truncated, 48_000, 30, 30, 1)
            .err()
            .unwrap();
        assert!(error.contains("out-point requires 48000 frames"), "{error}");
    }

    #[test]
    fn video_frame_count_sets_exact_audio_length_at_fractional_fps() {
        let project = Project::new("Duration", 320, 180, 29.97);
        let mix =
            OfflineMix::new(&project, &[], &HashMap::new(), 48_000, 300, 30_000, 1001).unwrap();
        assert_eq!(mix.total_frames(), 480_480);
        assert_eq!(mix.render_chunk(480_479, 1).unwrap(), [0.0, 0.0]);
        assert!(mix.render_chunk(480_480, 1).is_err());
    }

    #[test]
    fn unsupported_retime_or_missing_source_fails_closed() {
        let mut project = Project::new("Reject", 320, 180, 30.0);
        project.tracks.push(Track::new(1, "A"));
        let mut clip = Clip::new(1, 11, 1, 0, 1000, 0, 1000);
        let error = OfflineMix::new(
            &project,
            &[clip.clone()],
            &HashMap::new(),
            48_000,
            30,
            30,
            1,
        )
        .err()
        .unwrap();
        assert!(error.contains("missing"));
        clip.speed.default_value = 2.0;
        let sources = HashMap::from([(11, constant(48_000, 1, 48_000, [0.2, 0.0]))]);
        let error = OfflineMix::new(&project, &[clip], &sources, 48_000, 30, 30, 1)
            .err()
            .unwrap();
        assert!(error.contains("retiming"));
    }

    #[test]
    fn float_wav_fixture_round_trips_decoded_pcm() {
        let path = std::env::temp_dir().join(format!(
            "clipstrike-offline-mix-test-{}-{}.wav",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let samples = [0.0, 0.5, -0.25, 1.0, 0.75, -0.5];
        write_f32_wav(&path, 48_000, &samples).unwrap();
        let decoded = PcmSource::read_wav(&path).unwrap();
        assert_eq!(decoded.sample_rate, 48_000);
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.samples, samples);
        std::fs::remove_file(path).unwrap();
    }
}
