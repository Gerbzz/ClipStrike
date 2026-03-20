use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Copy, Default)]
pub enum EffectType {
    #[default]
    Brightness,
    Contrast,
    Saturation,
    Grayscale,
    BlurRadius,
    PixelateSize,
    Gamma,
    Hue,
    ChromaKey,
    Lut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TrackPoint {
    pub time_ms: i64,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MotionTrack {
    pub id: usize,
    pub name: String,
    pub points: Vec<TrackPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DetectedObject {
    pub label: String,
    pub confidence: f32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnalysisTask {
    Tracking(usize, i64, i64, [f32; 4]), // clip_id, start, end, initial_region [x, y, w, h]
    Detection(usize, i64),               // clip_id, time_ms
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AnalysisResponse {
    Tracking(MotionTrack),
    Detection(usize, Vec<DetectedObject>), // clip_id, objects
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Effect {
    pub id: usize,
    pub effect_type: EffectType,
    pub enabled: bool,
    // For now, we reuse the properties in Clip,
    // but in a more advanced version, each effect would have its own properties.
}
