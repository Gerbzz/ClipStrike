use crate::engine::VideoEngine;
use clipstrike_render::gpu::TexturePool;
use clipstrike_core::timeline::Clip;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Manages multiple video engines for concurrent clip playback.
pub struct EngineManager {
    engines: HashMap<usize, VideoEngine>, // Map clip_id -> Engine
    texture_pool: Arc<Mutex<TexturePool>>,
    queue: Arc<wgpu::Queue>,
}

impl EngineManager {
    pub fn new(texture_pool: Arc<Mutex<TexturePool>>, queue: Arc<wgpu::Queue>) -> Self {
        Self {
            engines: HashMap::new(),
            texture_pool,
            queue,
        }
    }

    /// Ensures an engine exists for the given clip and is prepared for the current time.
    pub fn sync_clip(&mut self, clip: &Clip, uri: &str) -> Result<bool, String> {
        if !self.engines.contains_key(&clip.id) {
            let engine = VideoEngine::new(
                uri,
                clip.id,
                Arc::clone(&self.texture_pool),
                Arc::clone(&self.queue),
                clip.video_enabled,
                clip.audio_enabled,
            )?;
            self.engines.insert(clip.id, engine);
            return Ok(true);
        }
        Ok(false)
    }

    pub fn play_clips(&self, active_ids: &[usize]) {
        for (clip_id, engine) in &self.engines {
            if active_ids.contains(clip_id) {
                let _ = engine.play();
            } else {
                let _ = engine.pause();
            }
        }
    }

    pub fn pause_all(&self) {
        for engine in self.engines.values() {
            let _ = engine.pause();
        }
    }

    pub fn seek_all(&self, timeline_ms: i64) {
        for engine in self.engines.values() {
            let _ = engine.seek(Duration::from_millis(timeline_ms.max(0) as u64));
        }
    }

    pub fn seek_clips(&self, clips: &[Clip], timeline_ms: i64) {
        for clip in clips {
            if let Some(engine) = self.engines.get(&clip.id) {
                let source_ms = clip.source_time_at(timeline_ms).max(0);
                let _ = engine.seek(Duration::from_millis(source_ms as u64));
            }
        }
    }

    pub fn set_clip_volume(&self, clip_id: usize, volume: f64) {
        if let Some(engine) = self.engines.get(&clip_id) {
            let _ = engine.set_volume(volume);
        }
    }

    pub fn remove_unused(&mut self, active_ids: &[usize]) {
        let removed_ids: Vec<usize> = self
            .engines
            .keys()
            .copied()
            .filter(|id| !active_ids.contains(id))
            .collect();

        for clip_id in &removed_ids {
            if let Some(engine) = self.engines.remove(clip_id) {
                let _ = engine.stop();
            }
        }

        if let Ok(mut pool) = self.texture_pool.lock() {
            for clip_id in removed_ids {
                pool.release_clip_frames(clip_id);
            }
        }
    }
}
