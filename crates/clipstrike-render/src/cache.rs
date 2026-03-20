use std::collections::HashMap;

pub struct CachedFrame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct VideoCache {
    pub frames: HashMap<i64, CachedFrame>,
    pub max_size: usize,
}

impl VideoCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            frames: HashMap::with_capacity(max_size),
            max_size,
        }
    }

    pub fn insert(&mut self, time_ms: i64, pixels: Vec<u8>, width: u32, height: u32) {
        if self.frames.len() >= self.max_size {
            // Simple eviction: remove the first one found (oldest/arbitrary)
            if let Some(&key) = self.frames.keys().next() {
                self.frames.remove(&key);
            }
        }
        self.frames.insert(
            time_ms,
            CachedFrame {
                pixels,
                width,
                height,
            },
        );
    }

    pub fn get(&self, time_ms: i64) -> Option<&CachedFrame> {
        self.frames.get(&time_ms)
    }

    pub fn clear(&mut self) {
        self.frames.clear();
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }
}
