use crate::engine::VideoEngine;
use clipstrike_audio::AudioHandle;
use clipstrike_core::timeline::Clip;
use clipstrike_render::gpu::FrameUploadRequest;
use std::collections::HashMap;
use std::time::Duration;

/// Manages multiple video engines for concurrent clip playback.
pub struct EngineManager {
    engines: HashMap<usize, VideoEngine>, // Map clip_id -> Engine
    frame_width: i32,
    frame_height: i32,
    #[cfg(not(target_arch = "wasm32"))]
    frame_sender: Option<tokio::sync::mpsc::Sender<FrameUploadRequest>>,
    audio_handle: Option<AudioHandle>,
}

impl EngineManager {
    pub fn new(
        frame_width: i32,
        frame_height: i32,
        #[cfg(not(target_arch = "wasm32"))] frame_sender: Option<
            tokio::sync::mpsc::Sender<FrameUploadRequest>,
        >,
        audio_handle: Option<AudioHandle>,
    ) -> Self {
        Self {
            engines: HashMap::new(),
            frame_width,
            frame_height,
            #[cfg(not(target_arch = "wasm32"))]
            frame_sender,
            audio_handle,
        }
    }

    /// Ensures an engine exists for the given clip and is prepared for the current time.
    pub fn sync_clip(&mut self, clip: &Clip, uri: &str) -> Result<bool, String> {
        if !self.engines.contains_key(&clip.id) {
            #[cfg(not(target_arch = "wasm32"))]
            let sender = self.frame_sender.clone();
            #[cfg(target_arch = "wasm32")]
            let sender = None;

            log::debug!(
                "EngineManager::sync_clip creating engine for clip_id={} uri={} video_enabled={} audio_enabled={}",
                clip.id,
                uri,
                clip.video_enabled,
                clip.audio_enabled
            );
            let engine = VideoEngine::new(
                uri,
                clip.id,
                self.frame_width,
                self.frame_height,
                clip.video_enabled,
                clip.audio_enabled,
                sender,
                self.audio_handle.clone(),
            )?;
            log::info!(
                "EngineManager::sync_clip created engine for clip_id={}",
                clip.id
            );
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

    pub fn get_audio_samples(&self, clip_id: usize) -> std::sync::Arc<Vec<f32>> {
        if let Some(engine) = self.engines.get(&clip_id) {
            engine.get_audio_samples()
        } else {
            std::sync::Arc::new(Vec::new())
        }
    }

    pub fn remove_clip_engine(&mut self, clip_id: usize) -> bool {
        if let Some(engine) = self.engines.remove(&clip_id) {
            let _ = engine.stop();
            true
        } else {
            false
        }
    }

    pub fn remove_unused(&mut self, active_ids: &[usize]) -> Vec<usize> {
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

        removed_ids
    }
}

#[cfg(test)]
mod tests {
    use super::EngineManager;

    #[test]
    fn remove_clip_engine_returns_false_when_engine_is_missing() {
        let mut manager = EngineManager::new(1920, 1080, None, None);

        assert!(!manager.remove_clip_engine(42));
    }
}
