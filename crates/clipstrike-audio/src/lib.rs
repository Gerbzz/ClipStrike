use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc;

pub mod waveform;

pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

#[derive(Debug)]
pub enum AudioCommand {
    SetPlaying(bool),
    SetPlayhead(i64),
    SetVolume(f32),
    UpdateSources(Vec<AudioSource>),
    PushSamples(usize, Vec<f32>), // clip_id, samples
}

#[derive(Debug)]
pub struct AudioSource {
    pub clip_id: usize,
    pub samples: Option<Arc<Vec<f32>>>, // None for streaming sources
    pub start_ms: i64,
    pub end_ms: i64,
    pub volume: clipstrike_core::keyframes::KeyframedProperty,
}

#[derive(Clone)]
pub struct AudioHandle {
    command_sender: mpsc::Sender<AudioCommand>,
    peak_levels: Arc<PeakLevels>,
}

impl AudioHandle {
    pub fn push_samples(&self, clip_id: usize, samples: Vec<f32>) {
        let _ = self
            .command_sender
            .try_send(AudioCommand::PushSamples(clip_id, samples));
    }

    pub fn set_sources(&self, sources: Vec<AudioSource>) {
        let _ = self
            .command_sender
            .try_send(AudioCommand::UpdateSources(sources));
    }

    pub fn set_playing(&self, playing: bool) {
        let _ = self
            .command_sender
            .try_send(AudioCommand::SetPlaying(playing));
    }

    pub fn set_playhead(&self, ms: i64) {
        let _ = self.command_sender.try_send(AudioCommand::SetPlayhead(ms));
    }

    pub fn set_volume(&self, volume: f32) {
        let _ = self
            .command_sender
            .try_send(AudioCommand::SetVolume(volume));
    }

    pub fn get_peak_levels(&self) -> [f32; 2] {
        self.peak_levels.load()
    }
}

struct PeakLevels {
    left: AtomicU32,
    right: AtomicU32,
}

impl PeakLevels {
    fn new() -> Self {
        Self {
            left: AtomicU32::new(0),
            right: AtomicU32::new(0),
        }
    }

    fn store(&self, peaks: [f32; 2]) {
        self.left.store(peaks[0].to_bits(), Ordering::Relaxed);
        self.right.store(peaks[1].to_bits(), Ordering::Relaxed);
    }

    fn load(&self) -> [f32; 2] {
        [
            f32::from_bits(self.left.load(Ordering::Relaxed)),
            f32::from_bits(self.right.load(Ordering::Relaxed)),
        ]
    }
}

#[derive(Default)]
struct StreamingBuffer {
    samples: VecDeque<f32>,
}

impl StreamingBuffer {
    fn push_samples(&mut self, samples: Vec<f32>) {
        self.samples.extend(samples);
    }

    fn pop_sample(&mut self) -> Option<f32> {
        self.samples.pop_front()
    }

    fn clear(&mut self) {
        self.samples.clear();
    }
}

pub fn decode_f32le_samples(bytes: &[u8]) -> Result<Vec<f32>, String> {
    if bytes.len() % std::mem::size_of::<f32>() != 0 {
        return Err(format!(
            "Audio buffer length {} is not a multiple of {} bytes",
            bytes.len(),
            std::mem::size_of::<f32>()
        ));
    }

    Ok(bytes
        .chunks_exact(std::mem::size_of::<f32>())
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

pub struct PlaybackEngine {
    _stream: cpal::Stream,
    handle: AudioHandle,
}

struct InnerPlaybackState {
    current_ms: i64,
    is_playing: bool,
    master_volume: f32,
    sources: Vec<AudioSource>,
    streaming_buffers: HashMap<usize, StreamingBuffer>,
    sample_rate: u32,
}

impl PlaybackEngine {
    pub fn new() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No output device available")?;

        let config = device.default_output_config().map_err(|e| e.to_string())?;

        let sample_rate = config.sample_rate().0;
        let peak_levels = Arc::new(PeakLevels::new());
        let peak_levels_ref = Arc::clone(&peak_levels);

        let mut inner = InnerPlaybackState {
            current_ms: 0,
            is_playing: false,
            master_volume: 1.0,
            sources: Vec::new(),
            streaming_buffers: HashMap::new(),
            sample_rate,
        };

        let (sender, receiver) = mpsc::channel(256);
        let mut receiver = receiver;

        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    while let Ok(cmd) = receiver.try_recv() {
                        match cmd {
                            AudioCommand::SetPlaying(p) => {
                                inner.is_playing = p;
                                if !p {
                                    peak_levels_ref.store([0.0, 0.0]);
                                }
                            }
                            AudioCommand::SetPlayhead(ms) => {
                                inner.current_ms = ms;
                                for buf in inner.streaming_buffers.values_mut() {
                                    buf.clear();
                                }
                            }
                            AudioCommand::SetVolume(v) => inner.master_volume = v,
                            AudioCommand::UpdateSources(s) => inner.sources = s,
                            AudioCommand::PushSamples(id, samples) => {
                                inner
                                    .streaming_buffers
                                    .entry(id)
                                    .or_default()
                                    .push_samples(samples);
                            }
                        }
                    }

                    if !inner.is_playing {
                        for sample in data.iter_mut() {
                            *sample = 0.0;
                        }
                        peak_levels_ref.store([0.0, 0.0]);
                        return;
                    }

                    let master_vol = inner.master_volume;
                    let current_ms = inner.current_ms;
                    let sr = inner.sample_rate as f32;

                    let mut current_peaks: [f32; 2] = [0.0, 0.0];

                    for (i, output_sample) in data.iter_mut().enumerate() {
                        let mut mixed = 0.0;
                        let offset_ms = (i as f32 * 500.0 / sr) as i64;
                        let sample_time_ms = current_ms + offset_ms;

                        for source in &inner.sources {
                            let clip_vol = source.volume.get_value_at(sample_time_ms);

                            if let Some(samples) = &source.samples {
                                if sample_time_ms >= source.start_ms
                                    && sample_time_ms < source.end_ms
                                {
                                    let source_offset_ms = sample_time_ms - source.start_ms;
                                    let sample_idx =
                                        (source_offset_ms as f32 * sr / 1000.0) as usize;
                                    if let Some(val) = samples.get(sample_idx) {
                                        mixed += val * clip_vol;
                                    }
                                }
                            } else if let Some(buf) =
                                inner.streaming_buffers.get_mut(&source.clip_id)
                            {
                                if let Some(sample) = buf.pop_sample() {
                                    mixed += sample * clip_vol;
                                }
                            }
                        }

                        let final_sample = (mixed * master_vol).clamp(-1.0, 1.0);
                        *output_sample = final_sample;

                        let chan = i % 2;
                        current_peaks[chan] = current_peaks[chan].max(final_sample.abs());
                    }

                    peak_levels_ref.store(current_peaks);

                    let ms_processed = (data.len() as f32 / (sr / 1000.0 * 2.0)) as i64;
                    inner.current_ms += ms_processed;
                },
                |err| log::error!("Audio stream error: {}", err),
                None,
            )
            .map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;

        Ok(Self {
            _stream: stream,
            handle: AudioHandle {
                command_sender: sender,
                peak_levels,
            },
        })
    }

    pub fn handle(&self) -> AudioHandle {
        self.handle.clone()
    }

    pub fn set_sources(&self, sources: Vec<AudioSource>) {
        self.handle.set_sources(sources);
    }

    pub fn set_playing(&self, playing: bool) {
        self.handle.set_playing(playing);
    }

    pub fn set_playhead(&self, ms: i64) {
        self.handle.set_playhead(ms);
    }

    pub fn set_volume(&self, volume: f32) {
        self.handle.set_volume(volume);
    }

    pub fn get_peak_levels(&self) -> [f32; 2] {
        self.handle.get_peak_levels()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_levels_round_trip_through_atomic_storage() {
        let peaks = PeakLevels::new();
        peaks.store([0.25, 0.75]);

        assert_eq!(peaks.load(), [0.25, 0.75]);
    }

    #[test]
    fn audio_handle_sends_commands_through_tokio_channel() {
        let (sender, mut receiver) = mpsc::channel(1);
        let handle = AudioHandle {
            command_sender: sender,
            peak_levels: Arc::new(PeakLevels::new()),
        };

        handle.set_volume(0.5);

        match receiver.try_recv() {
            Ok(AudioCommand::SetVolume(volume)) => assert_eq!(volume, 0.5),
            other => panic!("unexpected command result: {other:?}"),
        }
    }

    #[test]
    fn decode_f32le_samples_handles_misaligned_input_without_panicking() {
        let mut bytes = vec![0_u8];
        bytes.extend_from_slice(&1.5_f32.to_le_bytes());
        bytes.extend_from_slice(&(-2.25_f32).to_le_bytes());

        let decoded = decode_f32le_samples(&bytes[1..]).expect("misaligned buffer should decode");
        assert_eq!(decoded, vec![1.5, -2.25]);
    }

    #[test]
    fn decode_f32le_samples_rejects_partial_sample_payloads() {
        let err = decode_f32le_samples(&[1, 2, 3]).expect_err("partial sample should fail");
        assert!(err.contains("multiple of 4 bytes"));
    }

    #[test]
    fn streaming_buffer_consumes_samples_in_fifo_order() {
        let mut buffer = StreamingBuffer::default();
        buffer.push_samples(vec![0.25, -0.5, 0.75]);

        assert_eq!(buffer.pop_sample(), Some(0.25));
        assert_eq!(buffer.pop_sample(), Some(-0.5));
        assert_eq!(buffer.pop_sample(), Some(0.75));
        assert_eq!(buffer.pop_sample(), None);
    }

    #[test]
    fn streaming_buffer_clear_discards_pending_samples() {
        let mut buffer = StreamingBuffer::default();
        buffer.push_samples(vec![1.0, 2.0, 3.0]);
        buffer.clear();

        assert_eq!(buffer.pop_sample(), None);
        assert!(buffer.samples.is_empty());
    }
}
