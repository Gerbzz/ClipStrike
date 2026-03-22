use crate::engine::VideoEngine;
use clipstrike_audio::AudioHandle;
use clipstrike_core::timeline::Clip;
use clipstrike_render::gpu::FrameUploadRequest;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineEvent {
    EndOfStream { clip_id: usize },
}

fn playback_state_deltas(
    current: &HashSet<usize>,
    desired: &[usize],
) -> (Vec<usize>, Vec<usize>) {
    let desired: HashSet<usize> = desired.iter().copied().collect();
    let mut to_play: Vec<usize> = desired.difference(current).copied().collect();
    let mut to_pause: Vec<usize> = current.difference(&desired).copied().collect();
    to_play.sort_unstable();
    to_pause.sort_unstable();
    (to_play, to_pause)
}

/// Manages multiple video engines for concurrent clip playback.
pub struct EngineManager {
    engines: HashMap<usize, VideoEngine>, // Map clip_id -> Engine
    frame_width: i32,
    frame_height: i32,
    #[cfg(not(target_arch = "wasm32"))]
    frame_sender: Option<tokio::sync::mpsc::Sender<FrameUploadRequest>>,
    audio_handle: Option<AudioHandle>,
    event_sender: tokio::sync::mpsc::UnboundedSender<EngineEvent>,
    event_receiver: tokio::sync::mpsc::UnboundedReceiver<EngineEvent>,
    playing_clip_ids: HashSet<usize>,
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
        let (event_sender, event_receiver) = tokio::sync::mpsc::unbounded_channel();
        Self {
            engines: HashMap::new(),
            frame_width,
            frame_height,
            #[cfg(not(target_arch = "wasm32"))]
            frame_sender,
            audio_handle,
            event_sender,
            event_receiver,
            playing_clip_ids: HashSet::new(),
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
                Some(self.event_sender.clone()),
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

    pub fn play_clips(&mut self, active_ids: &[usize]) {
        let (to_play, to_pause) = playback_state_deltas(&self.playing_clip_ids, active_ids);

        for clip_id in &to_play {
            if let Some(engine) = self.engines.get(clip_id) {
                let _ = engine.play();
            }
        }

        for clip_id in &to_pause {
            if let Some(engine) = self.engines.get(clip_id) {
                let _ = engine.pause();
            }
        }

        self.playing_clip_ids = active_ids.iter().copied().collect();
    }

    pub fn pause_all(&mut self) {
        for clip_id in self.playing_clip_ids.drain() {
            if let Some(engine) = self.engines.get(&clip_id) {
                let _ = engine.pause();
            }
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

    pub fn seek_clips_interactive(&self, clips: &[Clip], timeline_ms: i64) {
        for clip in clips {
            if let Some(engine) = self.engines.get(&clip.id) {
                let source_ms = clip.source_time_at(timeline_ms).max(0);
                let _ = engine.seek_interactive(Duration::from_millis(source_ms as u64));
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

    /// Query GStreamer's actual playback position for a specific clip engine.
    /// Returns the source media position in milliseconds, or `None` if unavailable.
    pub fn query_clip_position_ms(&self, clip_id: usize) -> Option<i64> {
        self.engines.get(&clip_id)?.query_position_ms()
    }

    pub fn remove_clip_engine(&mut self, clip_id: usize) -> bool {
        self.playing_clip_ids.remove(&clip_id);
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
            self.playing_clip_ids.remove(clip_id);
            if let Some(engine) = self.engines.remove(clip_id) {
                let _ = engine.stop();
            }
        }

        removed_ids
    }

    pub fn drain_events(&mut self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_receiver.try_recv() {
            events.push(event);
        }
        events
    }

    #[cfg(test)]
    fn record_event_for_test(&self, event: EngineEvent) {
        let _ = self.event_sender.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::{playback_state_deltas, EngineEvent, EngineManager};

    #[test]
    fn remove_clip_engine_returns_false_when_engine_is_missing() {
        let mut manager = EngineManager::new(1920, 1080, None, None);

        assert!(!manager.remove_clip_engine(42));
    }

    #[test]
    fn drain_events_returns_buffered_engine_events() {
        let mut manager = EngineManager::new(1920, 1080, None, None);
        manager.record_event_for_test(EngineEvent::EndOfStream { clip_id: 7 });

        assert_eq!(
            manager.drain_events(),
            vec![EngineEvent::EndOfStream { clip_id: 7 }]
        );
    }


    #[test]
    fn playback_state_deltas_skip_redundant_engine_state_changes() {
        let current = std::collections::HashSet::from([1_usize, 2]);
        let (to_play, to_pause) = playback_state_deltas(&current, &[1, 2]);

        assert!(to_play.is_empty());
        assert!(to_pause.is_empty());
    }

    #[test]
    fn playback_state_deltas_only_changes_transitioning_clip_ids() {
        let current = std::collections::HashSet::from([1_usize, 2]);
        let (to_play, to_pause) = playback_state_deltas(&current, &[2, 3]);

        assert_eq!(to_play, vec![3]);
        assert_eq!(to_pause, vec![1]);
    }
}
