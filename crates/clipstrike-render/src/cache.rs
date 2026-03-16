use std::collections::HashMap;
use crate::gpu::FrameTexture;

pub struct VideoCache {
    pub frames: HashMap<i64, FrameTexture>,
    pub max_size: usize,
}

impl VideoCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            frames: HashMap::with_capacity(max_size),
            max_size,
        }
    }

    pub fn insert(&mut self, time_ms: i64, frame: FrameTexture) {
        if self.frames.len() >= self.max_size {
            // Very simple eviction: remove the first one found
            // In a real implementation, use LRU.
            if let Some(&key) = self.frames.keys().next() {
                self.frames.remove(&key);
            }
        }
        self.frames.insert(time_ms, frame);
    }

    pub fn get(&self, time_ms: i64) -> Option<&FrameTexture> {
        self.frames.get(&time_ms)
    }

    pub fn clear(&mut self) {
        self.frames.clear();
    }
}
