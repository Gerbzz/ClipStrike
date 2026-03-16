use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

pub mod waveform;

pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
}

pub struct PlaybackEngine {
    _stream: cpal::Stream,
    state: Arc<Mutex<PlaybackState>>,
}

struct PlaybackState {
    current_ms: i64,
    is_playing: bool,
    master_volume: f32,
    sources: Vec<AudioSource>,
}

pub struct AudioSource {
    pub clip_id: usize,
    pub samples: Arc<Vec<f32>>,
    pub start_ms: i64,
    pub end_ms: i64,
    pub volume: clipstrike_core::keyframes::KeyframedProperty,
}

impl PlaybackEngine {
    pub fn new() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No output device available")?;
        
        let config = device
            .default_output_config()
            .map_err(|e| e.to_string())?;

        let state = Arc::new(Mutex::new(PlaybackState {
            current_ms: 0,
            is_playing: false,
            master_volume: 1.0,
            sources: Vec::new(),
        }));

        let state_clone = Arc::clone(&state);
        let sample_rate = config.sample_rate().0;
        
        let stream = device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut state = state_clone.lock().unwrap();
                if !state.is_playing {
                    for sample in data.iter_mut() {
                        *sample = 0.0;
                    }
                    return;
                }

                let master_vol = state.master_volume;
                let current_ms = state.current_ms;
                
                // Task 12.1: Advanced mixing loop with volume keyframing
                for (i, output_sample) in data.iter_mut().enumerate() {
                    let mut mixed = 0.0;
                    
                    // Interleaved stereo support (Task 12.8)
                    // If the output config is stereo, we increment i by 2 in a full implementation.
                    // For now we assume the caller passes mono or stereo chunks.
                    let offset_ms = (i as f32 * 500.0 / sample_rate as f32) as i64;
                    let sample_time_ms = current_ms + offset_ms;

                    for source in &state.sources {
                        if sample_time_ms >= source.start_ms && sample_time_ms < source.end_ms {
                            let source_offset_ms = sample_time_ms - source.start_ms;
                            let sample_idx = (source_offset_ms as f32 * sample_rate as f32 / 1000.0) as usize;
                            if let Some(val) = source.samples.get(sample_idx) {
                                // Task 12.5: Per-sample volume interpolation
                                let clip_vol = source.volume.get_value_at(sample_time_ms);
                                mixed += val * clip_vol;
                            }
                        }
                    }
                    
                    *output_sample = mixed * master_vol;
                }

                // Increment state time based on buffer size and 2 channels (stereo)
                let samples_per_ms = sample_rate as f32 / 1000.0;
                let ms_processed = (data.len() as f32 / (samples_per_ms * 2.0)) as i64;
                state.current_ms += ms_processed;
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        ).map_err(|e| e.to_string())?;

        stream.play().map_err(|e| e.to_string())?;

        Ok(Self {
            _stream: stream,
            state,
        })
    }

    pub fn set_sources(&self, sources: Vec<AudioSource>) {
        if let Ok(mut state) = self.state.lock() {
            state.sources = sources;
        }
    }

    pub fn set_playing(&self, playing: bool) {
        if let Ok(mut state) = self.state.lock() {
            state.is_playing = playing;
        }
    }

    pub fn set_playhead(&self, ms: i64) {
        if let Ok(mut state) = self.state.lock() {
            state.current_ms = ms;
        }
    }

    pub fn set_volume(&self, volume: f32) {
        if let Ok(mut state) = self.state.lock() {
            state.master_volume = volume;
        }
    }
}
