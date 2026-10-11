//! Conservative importer for the original ClipStrike `.cstrike` JSON project.
//! Preflight rejects anything whose picture/sound semantics cannot be carried into ClipStrike.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use filmcraft_engine::Session;
use filmcraft_project::{ClipId, ItemId};
use filmcraft_time::{FrameRate, Tick};
use serde_json::{Map, Value, json};

const MAX_PROJECT_BYTES: u64 = 1_048_576;
const MAX_TIME_MS: i64 = 86_400_000;

#[derive(Debug, Clone)]
pub struct Refusal {
    pub reasons: Vec<String>,
}

impl Refusal {
    fn one(message: impl Into<String>) -> Self {
        Self { reasons: vec![message.into()] }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Legacy ClipStrike project conversion refused; no converted document was created:")?;
        for reason in &self.reasons {
            writeln!(f, "- {reason}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Asset {
    id: u64,
    name: String,
    path: PathBuf,
    duration_ms: i64,
    source: Value,
}

#[derive(Debug, Clone)]
struct Track {
    id: u64,
    name: String,
    visible: bool,
    audio_enabled: bool,
    locked: bool,
}

#[derive(Debug, Clone)]
struct Clip {
    id: u64,
    asset_id: u64,
    track_id: u64,
    start_ms: i64,
    end_ms: i64,
    in_ms: i64,
    out_ms: i64,
    video: bool,
    audio: bool,
    volume: f64,
    pos_x: f64,
    pos_y: f64,
    scale_x: f64,
    scale_y: f64,
    opacity: f64,
}

#[derive(Debug, Clone)]
struct Plan {
    name: String,
    width: u32,
    height: u32,
    fps: f64,
    master_gain_db: f64,
    tracks: Vec<Track>,
    assets: Vec<Asset>,
    clips: Vec<Clip>,
    original: Value,
}

fn obj<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>, Refusal> {
    value.as_object().ok_or_else(|| Refusal::one(format!("{context} must be a JSON object")))
}

fn key<'a>(map: &'a Map<String, Value>, name: &str, context: &str) -> Result<&'a Value, Refusal> {
    map.get(name).ok_or_else(|| Refusal::one(format!("{context}.{name} is missing")))
}

fn text(map: &Map<String, Value>, name: &str, context: &str) -> Result<String, Refusal> {
    key(map, name, context)?.as_str().map(str::to_owned).ok_or_else(|| Refusal::one(format!("{context}.{name} must be text")))
}

fn integer(map: &Map<String, Value>, name: &str, context: &str) -> Result<i64, Refusal> {
    key(map, name, context)?.as_i64().ok_or_else(|| Refusal::one(format!("{context}.{name} must be an integer")))
}

fn positive_id(map: &Map<String, Value>, name: &str, context: &str) -> Result<u64, Refusal> {
    let n = key(map, name, context)?.as_u64().ok_or_else(|| Refusal::one(format!("{context}.{name} must be a positive ID")))?;
    if n == 0 { Err(Refusal::one(format!("{context}.{name} must be a positive ID"))) } else { Ok(n) }
}

fn number(map: &Map<String, Value>, name: &str, context: &str) -> Result<f64, Refusal> {
    let n = key(map, name, context)?.as_f64().ok_or_else(|| Refusal::one(format!("{context}.{name} must be numeric")))?;
    if n.is_finite() { Ok(n) } else { Err(Refusal::one(format!("{context}.{name} must be finite"))) }
}

fn flag(map: &Map<String, Value>, name: &str, context: &str) -> Result<bool, Refusal> {
    key(map, name, context)?.as_bool().ok_or_else(|| Refusal::one(format!("{context}.{name} must be true or false")))
}

fn array<'a>(map: &'a Map<String, Value>, name: &str, context: &str) -> Result<&'a Vec<Value>, Refusal> {
    key(map, name, context)?.as_array().ok_or_else(|| Refusal::one(format!("{context}.{name} must be an array")))
}

fn check_keys(map: &Map<String, Value>, allowed: &[&str], context: &str, reasons: &mut Vec<String>) {
    for name in map.keys() {
        if !allowed.contains(&name.as_str()) {
            reasons.push(format!("{context}.{name} is not supported; its meaning would be lost"));
        }
    }
}

fn reject_nonempty(map: &Map<String, Value>, name: &str, context: &str, reasons: &mut Vec<String>) {
    if map.get(name).is_some_and(|v| !v.as_array().is_some_and(Vec::is_empty)) {
        reasons.push(format!("{context}.{name} is not empty; conversion of this structure is not implemented"));
    }
}

fn reject_true(map: &Map<String, Value>, name: &str, context: &str, reasons: &mut Vec<String>) {
    if map.get(name).is_some_and(|v| v != &Value::Bool(false)) {
        reasons.push(format!("{context}.{name} must be false; this feature is not supported"));
    }
}

fn static_property(map: &Map<String, Value>, name: &str, default: f64, context: &str, reasons: &mut Vec<String>) -> f64 {
    let Some(value) = map.get(name) else { return default };
    let Some(property) = value.as_object() else {
        reasons.push(format!("{context}.{name} is not a property"));
        return default;
    };
    check_keys(property, &["keyframes", "default_value"], &format!("{context}.{name}"), reasons);
    if !property.get("keyframes").is_some_and(|v| v.as_array().is_some_and(Vec::is_empty)) {
        reasons.push(format!("{context}.{name} has keyframes; their interpolation cannot yet be preserved"));
    }
    match property.get("default_value").and_then(Value::as_f64) {
        Some(v) if v.is_finite() => v,
        _ => {
            reasons.push(format!("{context}.{name}.default_value must be finite"));
            default
        }
    }
}

fn unsupported_property(map: &Map<String, Value>, name: &str, default: f64, context: &str, reasons: &mut Vec<String>) {
    let value = static_property(map, name, default, context, reasons);
    if (value - default).abs() > 0.000_001 {
        reasons.push(format!("{context}.{name} has a non-default effect ({value}); conversion is not implemented"));
    }
}

fn rate_for(fps: f64) -> Option<FrameRate> {
    if fps == 24.0 {
        Some(FrameRate::FPS_24)
    } else if fps == 25.0 {
        Some(FrameRate::FPS_25)
    } else if fps == 30.0 {
        Some(FrameRate::FPS_30)
    } else if fps == 50.0 {
        Some(FrameRate::FPS_50)
    } else if fps == 60.0 {
        Some(FrameRate::FPS_60)
    } else {
        None
    }
}

fn ticks(ms: i64) -> Tick {
    Tick::from_units(ms, 1_000)
}

fn resolve_path(uri: &str, base: &Path) -> Result<PathBuf, Refusal> {
    let path = if let Some(rest) = uri.strip_prefix("file:///") {
        if cfg!(windows) { rest.to_owned() } else { format!("/{rest}") }
    } else if uri.contains("://") {
        return Err(Refusal::one(format!("unsupported media URI scheme: {uri}")));
    } else {
        uri.to_owned()
    };
    let mut bytes = Vec::with_capacity(path.len());
    let raw = path.as_bytes();
    let mut at = 0;
    while at < raw.len() {
        if raw[at] == b'%' {
            let pair = raw.get(at + 1..at + 3).ok_or_else(|| Refusal::one(format!("invalid percent escape in media URI: {uri}")))?;
            let hex = std::str::from_utf8(pair)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or_else(|| Refusal::one(format!("invalid percent escape in media URI: {uri}")))?;
            bytes.push(hex);
            at += 3;
        } else {
            bytes.push(raw[at]);
            at += 1;
        }
    }
    let decoded = String::from_utf8(bytes).map_err(|_| Refusal::one(format!("media URI is not UTF-8: {uri}")))?;
    let path = PathBuf::from(decoded);
    Ok(if path.is_absolute() { path } else { base.join(path) })
}

fn plan_str(source: &str, base: &Path) -> Result<Plan, Refusal> {
    let original: Value = serde_json::from_str(source).map_err(|e| Refusal::one(format!("invalid .cstrike JSON: {e}")))?;
    let root = obj(&original, "project")?;
    let mut reasons = Vec::new();
    check_keys(
        root,
        &[
            "name",
            "width",
            "height",
            "framerate",
            "master_volume",
            "tracks",
            "assets",
            "capture_sources",
            "titles",
            "generators",
            "transition_masks",
            "clips",
            "transitions",
            "markers",
            "watermark_asset_id",
            "watermark_opacity",
            "watermark_scale",
            "watermark_pos_x",
            "watermark_pos_y",
        ],
        "project",
        &mut reasons,
    );
    for field in ["titles", "generators", "transition_masks", "transitions", "markers"] {
        reject_nonempty(root, field, "project", &mut reasons);
    }
    if root.get("watermark_asset_id").is_some_and(|v| !v.is_null()) {
        reasons.push("project.watermark_asset_id is set; watermark conversion is not implemented".into());
    }
    let name = text(root, "name", "project")?;
    let width = u32::try_from(integer(root, "width", "project")?)
        .ok()
        .filter(|n| (1..=8192).contains(n))
        .ok_or_else(|| Refusal::one("project.width must be positive"))?;
    let height = u32::try_from(integer(root, "height", "project")?)
        .ok()
        .filter(|n| (1..=8192).contains(n))
        .ok_or_else(|| Refusal::one("project.height must be positive"))?;
    let fps = number(root, "framerate", "project")?;
    let rate = rate_for(fps).ok_or_else(|| Refusal::one(format!("project.framerate {fps} is unsupported for exact conversion")))?;
    let master = number(root, "master_volume", "project")?;
    if !(0.000_015_848_931_924_611_14..=251.188_643_150_958).contains(&master) {
        reasons.push("project.master_volume is outside ClipStrike's exact +48 to -96 dB range".into());
    }
    let tracks_json = array(root, "tracks", "project")?;
    if tracks_json.is_empty() || tracks_json.len() > 256 {
        reasons.push("project.tracks must contain 1–256 tracks".into());
    }
    let mut tracks = Vec::new();
    let mut track_ids = BTreeSet::new();
    for (index, value) in tracks_json.iter().enumerate() {
        let context = format!("track[{index}]");
        let tr = obj(value, &context)?;
        check_keys(tr, &["id", "name", "visible", "audio_enabled", "solo", "locked"], &context, &mut reasons);
        let id = positive_id(tr, "id", &context)?;
        if !track_ids.insert(id) {
            reasons.push(format!("{context}.id {id} is duplicated"));
        }
        if flag(tr, "solo", &context)? {
            reasons.push(format!("{context}.solo is enabled; solo/mute interaction is not yet mapped"));
        }
        tracks.push(Track {
            id,
            name: text(tr, "name", &context)?,
            visible: flag(tr, "visible", &context)?,
            audio_enabled: flag(tr, "audio_enabled", &context)?,
            locked: flag(tr, "locked", &context)?,
        });
    }
    let expected: BTreeSet<u64> = (1..=u64::try_from(tracks.len()).unwrap_or(0)).collect();
    if track_ids != expected {
        reasons.push("track IDs must be contiguous from 1 to preserve layer order".into());
    }
    let captures = root.get("capture_sources").and_then(Value::as_array);
    if root.get("capture_sources").is_some() && captures.is_none() {
        reasons.push("project.capture_sources must be an array".into());
    }
    let mut assets = Vec::new();
    let mut asset_ids = BTreeSet::new();
    let mut asset_paths = BTreeSet::new();
    for (index, value) in array(root, "assets", "project")?.iter().enumerate() {
        let context = format!("asset[{index}]");
        let asset = obj(value, &context)?;
        check_keys(asset, &["id", "name", "uri", "duration_ms", "audio_levels"], &context, &mut reasons);
        let id = positive_id(asset, "id", &context)?;
        if !asset_ids.insert(id) {
            reasons.push(format!("{context}.id {id} is duplicated"));
        }
        let uri = text(asset, "uri", &context)?;
        let path = resolve_path(&uri, base)?;
        if !asset_paths.insert(path.clone()) {
            reasons.push(format!("{context}.uri duplicates another asset path; distinct source-item metadata could be lost"));
        }
        let duration_ms = integer(asset, "duration_ms", &context)?;
        if !(1..=MAX_TIME_MS).contains(&duration_ms) {
            reasons.push(format!("{context}.duration_ms must be between 1 ms and 24 hours"));
        }
        if !asset.get("audio_levels").is_some_and(Value::is_array) {
            reasons.push(format!("{context}.audio_levels must be an array"));
        }
        assets.push(Asset { id, name: text(asset, "name", &context)?, path, duration_ms, source: value.clone() });
    }
    if let Some(captures) = captures {
        for (index, capture) in captures.iter().enumerate() {
            let context = format!("capture_sources[{index}]");
            let capture = obj(capture, &context)?;
            let id = positive_id(capture, "asset_id", &context)?;
            if !asset_ids.contains(&id) {
                reasons.push(format!("{context}.asset_id {id} has no matching asset"));
            }
        }
    }
    let mut clips = Vec::new();
    let mut clip_ids = BTreeSet::new();
    for (index, value) in array(root, "clips", "project")?.iter().enumerate() {
        let context = format!("clip[{index}]");
        let clip = obj(value, &context)?;
        check_keys(
            clip,
            &[
                "id",
                "asset_id",
                "track_id",
                "start",
                "end",
                "in_point",
                "out_point",
                "blend_mode",
                "brightness",
                "contrast",
                "saturation",
                "grayscale",
                "blur_radius",
                "volume",
                "pos_x",
                "pos_y",
                "scale_x",
                "scale_y",
                "rotation",
                "opacity",
                "speed",
                "pixelate_size",
                "gamma",
                "hue",
                "color_tint_r",
                "color_tint_g",
                "color_tint_b",
                "chroma_key_threshold",
                "chroma_key_fuzziness",
                "chroma_key_r",
                "chroma_key_g",
                "chroma_key_b",
                "lut_enabled",
                "lut_path",
                "lut_size",
                "reverse",
                "freeze_frame_ms",
                "video_enabled",
                "audio_enabled",
                "effects",
                "stabilization_enabled",
                "motion_tracking_enabled",
                "stabilization_strength",
                "object_detection_enabled",
                "tracking_data",
                "detections",
                "group_id",
                "color_tag",
            ],
            &context,
            &mut reasons,
        );
        let id = positive_id(clip, "id", &context)?;
        if !clip_ids.insert(id) {
            reasons.push(format!("{context}.id {id} is duplicated"));
        }
        let asset_id = positive_id(clip, "asset_id", &context)?;
        let track_id = positive_id(clip, "track_id", &context)?;
        if !asset_ids.contains(&asset_id) {
            reasons.push(format!("{context}.asset_id {asset_id} has no matching asset"));
        }
        if !track_ids.contains(&track_id) {
            reasons.push(format!("{context}.track_id {track_id} has no matching track"));
        }
        let start_ms = integer(clip, "start", &context)?;
        let end_ms = integer(clip, "end", &context)?;
        let in_ms = integer(clip, "in_point", &context)?;
        let out_ms = integer(clip, "out_point", &context)?;
        let timeline_len = end_ms.checked_sub(start_ms);
        let source_len = out_ms.checked_sub(in_ms);
        if ![start_ms, end_ms, in_ms, out_ms].iter().all(|ms| (0..=MAX_TIME_MS).contains(ms))
            || timeline_len.is_none_or(|n| n <= 0)
            || timeline_len != source_len
        {
            reasons.push(format!("{context} has invalid, overlong or speed-adjusted trim bounds"));
        }
        for (field, ms) in [("start", start_ms), ("end", end_ms), ("in_point", in_ms), ("out_point", out_ms)] {
            if rate.snap_nearest(ticks(ms)) != ticks(ms) {
                reasons.push(format!("{context}.{field} at {ms} ms is off the {fps} fps frame grid"));
            }
        }
        if let Some(asset) = assets.iter().find(|a| a.id == asset_id) {
            if out_ms > asset.duration_ms {
                reasons.push(format!("{context}.out_point exceeds asset {asset_id} duration"));
            }
        }
        if clip.get("blend_mode").and_then(Value::as_str) != Some("Normal") {
            reasons.push(format!("{context}.blend_mode is unsupported"));
        }
        for field in ["effects", "tracking_data", "detections"] {
            reject_nonempty(clip, field, &context, &mut reasons);
        }
        for field in ["reverse", "stabilization_enabled", "motion_tracking_enabled", "object_detection_enabled"] {
            reject_true(clip, field, &context, &mut reasons);
        }
        for field in ["freeze_frame_ms", "lut_path", "group_id"] {
            if clip.get(field).is_some_and(|v| !v.is_null()) {
                reasons.push(format!("{context}.{field} is set; conversion is unsupported"));
            }
        }
        if clip.get("lut_size").is_some_and(|v| v.as_u64() != Some(0)) {
            reasons.push(format!("{context}.lut_size is nonzero"));
        }
        for (field, default) in [
            ("brightness", 1.0),
            ("contrast", 1.0),
            ("saturation", 1.0),
            ("grayscale", 0.0),
            ("blur_radius", 0.0),
            ("rotation", 0.0),
            ("pixelate_size", 1.0),
            ("gamma", 1.0),
            ("hue", 0.0),
            ("color_tint_r", 1.0),
            ("color_tint_g", 1.0),
            ("color_tint_b", 1.0),
            ("chroma_key_threshold", 0.0),
            ("chroma_key_fuzziness", 0.1),
            ("chroma_key_r", 0.0),
            ("chroma_key_g", 1.0),
            ("chroma_key_b", 0.0),
            ("lut_enabled", 0.0),
            ("stabilization_strength", 0.0),
            ("speed", 1.0),
        ] {
            unsupported_property(clip, field, default, &context, &mut reasons);
        }
        let volume = static_property(clip, "volume", 1.0, &context, &mut reasons);
        let pos_x = static_property(clip, "pos_x", 0.0, &context, &mut reasons);
        let pos_y = static_property(clip, "pos_y", 0.0, &context, &mut reasons);
        let scale_x = static_property(clip, "scale_x", 1.0, &context, &mut reasons);
        let scale_y = static_property(clip, "scale_y", 1.0, &context, &mut reasons);
        let opacity = static_property(clip, "opacity", 1.0, &context, &mut reasons);
        if !(0.000_015_848_931_924_611_14..=63_095.734_448_019_33).contains(&volume) {
            reasons.push(format!("{context}.volume is outside ClipStrike's exact ±96 dB range"));
        }
        if scale_x <= 0.0 || scale_y <= 0.0 || scale_x > 100.0 || scale_y > 100.0 {
            reasons.push(format!("{context}.scale_x/scale_y is outside ClipStrike's supported scale range"));
        }
        if !(0.0..=1.0).contains(&opacity) {
            reasons.push(format!("{context}.opacity must be 0–1"));
        }
        if pos_x.abs() > 100_000.0 || pos_y.abs() > 100_000.0 {
            reasons.push(format!("{context}.pos_x/pos_y exceeds the safe pixel translation range"));
        }
        let video = flag(clip, "video_enabled", &context)?;
        let audio = flag(clip, "audio_enabled", &context)?;
        if !video && !audio {
            reasons.push(format!("{context} disables both video and audio"));
        }
        clips.push(Clip { id, asset_id, track_id, start_ms, end_ms, in_ms, out_ms, video, audio, volume, pos_x, pos_y, scale_x, scale_y, opacity });
    }
    clips.sort_by_key(|c| (c.track_id, c.start_ms, c.id));
    for pair in clips.windows(2) {
        if pair[0].track_id == pair[1].track_id && pair[0].end_ms > pair[1].start_ms {
            reasons.push(format!("clips {} and {} overlap on track {}; ClipStrike placement would overwrite one", pair[0].id, pair[1].id, pair[0].track_id));
        }
    }
    if !reasons.is_empty() {
        return Err(Refusal { reasons });
    }
    Ok(Plan { name, width, height, fps, master_gain_db: 20.0 * master.log10(), tracks, assets, clips, original })
}

fn step_error(step: &str, error: impl fmt::Display) -> Refusal {
    Refusal::one(format!("{step}: {error}"))
}

fn place_params(clip: &Clip, video: bool, audio: bool, item: u64) -> Value {
    let mut params = json!({
        "item": item,
        "time": ticks(clip.start_ms).0,
        "sourceIn": ticks(clip.in_ms).0,
        "duration": ticks(clip.end_ms - clip.start_ms).0,
        "insert": false,
        "video": video,
        "audio": audio,
    });
    if video {
        params["track"] = json!(format!("V{}", clip.track_id));
    }
    if audio {
        params["audioTrack"] = json!(format!("A{}", clip.track_id));
    }
    params
}

fn set_effect(session: &mut Session, clip: ClipId, effect: &str, param: &str, value: Value) -> Result<(), Refusal> {
    session
        .execute("effects.setParam", json!({"clip": clip.0, "effect": effect, "param": param, "value": value}))
        .map_err(|e| step_error(&format!("clip {} {effect}.{param}", clip.0), e))?;
    Ok(())
}

fn require_requested_streams(clip: &Clip, has_video: bool, has_audio: bool) -> Result<(), Refusal> {
    if clip.video && !has_video {
        return Err(Refusal::one(format!(
            "clip {} requests video, but asset {} has no video stream; disable video for this clip or relink media with video",
            clip.id, clip.asset_id
        )));
    }
    if clip.audio && !has_audio {
        return Err(Refusal::one(format!(
            "clip {} requests audio, but asset {} has no audio stream; disable audio for this clip or relink media with audio",
            clip.id, clip.asset_id
        )));
    }
    Ok(())
}

fn convert(plan: Plan) -> Result<Session, Refusal> {
    // Work in a new, unsaved session. Any failed import/edit discards it; neither the
    // current editor session nor the original .cstrike file can be changed here.
    for asset in &plan.assets {
        if !asset.path.is_file() {
            return Err(Refusal::one(format!("asset {} media is missing: {}", asset.id, asset.path.display())));
        }
    }
    let mut session = Session::default();
    // Probe every requested stream before constructing the converted project. `probe_file`
    // reads a container index through the bounded random-access reader. If the file changes
    // between this preflight and import, the same check runs again before timeline placement.
    for asset in &plan.assets {
        let source = session
            .media
            .probe_file(&asset.path.to_string_lossy(), &*session.services)
            .map_err(|e| step_error(&format!("asset {} media probe", asset.id), e))?;
        let info = source.info();
        for clip in plan.clips.iter().filter(|clip| clip.asset_id == asset.id) {
            require_requested_streams(clip, info.has_video(), info.has_audio())?;
        }
    }
    session.prefs.media.default_media_scaling = "none".into();
    session.prefs.media_analysis.auto_transcribe = false;
    session.execute("file.newProject", json!({"name": plan.name.clone()})).map_err(|e| step_error("new project", e))?;
    session
        .execute(
            "file.newSequence",
            json!({
                "name": plan.name.clone(),
                "width": plan.width,
                "height": plan.height,
                "fps": plan.fps,
                "video": plan.tracks.len(),
                "audio": plan.tracks.len(),
            }),
        )
        .map_err(|e| step_error("new sequence", e))?;
    let mut imported = BTreeMap::new();
    for asset in &plan.assets {
        let result = session
            .execute("file.import", json!({"paths": [asset.path.to_string_lossy()]}))
            .map_err(|e| step_error(&format!("asset {} import", asset.id), e))?;
        let ids = result.get("items").and_then(Value::as_array).ok_or_else(|| Refusal::one(format!("asset {} import returned no items", asset.id)))?;
        if ids.len() != 1 || result.get("errors").and_then(Value::as_array).is_some_and(|v| !v.is_empty()) {
            return Err(Refusal::one(format!("asset {} import was partial: {result}", asset.id)));
        }
        let id = ids.first().and_then(Value::as_u64).ok_or_else(|| Refusal::one(format!("asset {} import returned an invalid ID", asset.id)))?;
        let item = session.project.item(ItemId(id)).ok_or_else(|| Refusal::one(format!("asset {} import item disappeared", asset.id)))?;
        let media = item.as_media().ok_or_else(|| Refusal::one(format!("asset {} did not import as media", asset.id)))?;
        if let Some(video) = &media.info.video {
            if video.width != plan.width || video.height != plan.height {
                return Err(Refusal::one(format!(
                    "asset {} is {}x{} but the legacy sequence is {}x{}; source scaling needs a verified mapping",
                    asset.id, video.width, video.height, plan.width, plan.height
                )));
            }
        }
        // The legacy asset duration may include container/audio padding beyond the
        // last usable video frame. Verify every clip's actual out point below;
        // requiring this metadata duration would refuse valid trimmed media.
        let captures: Vec<Value> = plan
            .original
            .get("capture_sources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|v| v.get("asset_id").and_then(Value::as_u64) == Some(asset.id))
            .cloned()
            .collect();
        let name = asset.name.clone();
        let source = asset.source.to_string();
        session
            .edit("Legacy Media Metadata", move |project, _| {
                let item = project.item_mut(ItemId(id)).ok_or_else(|| filmcraft_engine::EngineError::Other("import item disappeared".into()))?;
                item.name = name;
                item.metadata.insert("clipstrike.asset".into(), source);
                item.metadata.insert("clipstrike.capture_sources".into(), json!(captures).to_string());
                Ok(())
            })
            .map_err(|e| step_error(&format!("asset {} metadata", asset.id), e))?;
        imported.insert(asset.id, id);
    }
    for clip in &plan.clips {
        let item = *imported.get(&clip.asset_id).ok_or_else(|| Refusal::one(format!("clip {} asset {} was not imported", clip.id, clip.asset_id)))?;
        let media =
            session.project.item(ItemId(item)).and_then(|i| i.as_media()).ok_or_else(|| Refusal::one(format!("clip {} media item disappeared", clip.id)))?;
        if ticks(clip.out_ms) > media.duration() {
            return Err(Refusal::one(format!("clip {} out point exceeds the probed media duration", clip.id)));
        }
        require_requested_streams(clip, media.info.video.is_some(), media.info.has_audio())?;
        let (video, audio) = (clip.video, clip.audio);
        let result =
            session.execute("timeline.place", place_params(clip, video, audio, item)).map_err(|e| step_error(&format!("clip {} placement", clip.id), e))?;
        let ids = result.get("clips").and_then(Value::as_array).ok_or_else(|| Refusal::one(format!("clip {} placement returned no clips", clip.id)))?;
        if ids.len() != usize::from(video) + usize::from(audio) {
            return Err(Refusal::one(format!("clip {} placement changed the number of streams; channel mapping needs review", clip.id)));
        }
        let seq = session.active_sequence().ok_or_else(|| Refusal::one("the converted sequence disappeared"))?;
        let video_id = seq.video_tracks.iter().flat_map(|t| &t.items).find(|i| ids.iter().any(|v| v.as_u64() == Some(i.id.0))).map(|i| i.id);
        let audio_id = seq.audio_tracks.iter().flat_map(|t| &t.items).find(|i| ids.iter().any(|v| v.as_u64() == Some(i.id.0))).map(|i| i.id);
        if video != video_id.is_some() || audio != audio_id.is_some() {
            return Err(Refusal::one(format!("clip {} streams landed on unexpected tracks", clip.id)));
        }
        for id in [video_id, audio_id].into_iter().flatten() {
            let item = session
                .active_sequence()
                .and_then(|q| q.find_item(id))
                .map(|(_, item)| item)
                .ok_or_else(|| Refusal::one(format!("clip {} vanished after placement", clip.id)))?;
            if item.start != ticks(clip.start_ms) || item.duration != ticks(clip.end_ms - clip.start_ms) || item.source_in != ticks(clip.in_ms) {
                return Err(Refusal::one(format!("clip {} was rounded or moved by timeline placement", clip.id)));
            }
        }
        if let Some(id) = video_id {
            // Legacy shader: output pixel = scale * source pixel + translation.
            // ClipStrike's Motion uses position + scale * (source pixel - anchor).
            // Anchor at (0,0) makes the two static transforms identical.
            set_effect(&mut session, id, "motion", "anchor", json!([0.0, 0.0]))?;
            set_effect(&mut session, id, "motion", "position", json!([clip.pos_x, clip.pos_y]))?;
            set_effect(&mut session, id, "motion", "uniform_scale", json!(false))?;
            set_effect(&mut session, id, "motion", "scale", json!(clip.scale_y * 100.0))?;
            set_effect(&mut session, id, "motion", "scale_width", json!(clip.scale_x * 100.0))?;
            set_effect(&mut session, id, "opacity", "opacity", json!(clip.opacity * 100.0))?;
        }
        if let Some(id) = audio_id {
            let db = 20.0 * clip.volume.log10();
            session
                .execute("clip.audioGain", json!({"clips": [id.0], "mode": "set", "db": db}))
                .map_err(|e| step_error(&format!("clip {} audio gain", clip.id), e))?;
        }
    }
    let master_gain_db = plan.master_gain_db;
    session
        .edit_sequence("Legacy Mix Level", move |seq, _, _| {
            seq.master_volume_db = master_gain_db;
            Ok(())
        })
        .map_err(|e| step_error("master volume", e))?;
    for track in &plan.tracks {
        session
            .execute(
                "timeline.setTrack",
                json!({"track": format!("V{}", track.id), "name": track.name.clone(), "enabled": track.visible, "locked": track.locked}),
            )
            .map_err(|e| step_error(&format!("video track {}", track.id), e))?;
        session
            .execute(
                "timeline.setTrack",
                json!({"track": format!("A{}", track.id), "name": track.name.clone(), "muted": !track.audio_enabled, "locked": track.locked}),
            )
            .map_err(|e| step_error(&format!("audio track {}", track.id), e))?;
    }
    let notes = format!("ClipStrike legacy source JSON retained for provenance. Original .cstrike is unchanged.\\n{}", plan.original);
    session
        .edit("Legacy Source Provenance", move |project, _| {
            project.notes = notes;
            Ok(())
        })
        .map_err(|e| step_error("source provenance", e))?;
    Ok(session)
}

pub fn load_and_convert(path: &Path) -> Result<Session, Refusal> {
    let size = std::fs::metadata(path).map_err(|e| step_error("read legacy project", e))?.len();
    if size > MAX_PROJECT_BYTES {
        return Err(Refusal::one("legacy project exceeds the 1 MiB safe import limit"));
    }
    let source = std::fs::read_to_string(path).map_err(|e| step_error("read legacy project", e))?;
    if source.len() as u64 > MAX_PROJECT_BYTES {
        return Err(Refusal::one("legacy project changed while reading and exceeds 1 MiB"));
    }
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let plan = plan_str(&source, base)?;
    convert(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTHETIC: &str = include_str!("../tests/fixtures/synthetic-chronological.cstrike");

    #[test]
    fn synthetic_timeline_maps_without_rounded_cuts() {
        let plan = plan_str(SYNTHETIC, Path::new("fixtures")).unwrap();
        assert_eq!((plan.width, plan.height, plan.fps), (1920, 1080, 60.0));
        assert_eq!((plan.assets.len(), plan.clips.len()), (3, 2));
        assert_eq!((plan.clips[0].start_ms, plan.clips[0].end_ms, plan.clips[0].out_ms), (0, 1000, 1000));
        assert_eq!((plan.clips[1].start_ms, plan.clips[1].end_ms, plan.clips[1].out_ms), (1000, 2200, 1200));
        let p = place_params(&plan.clips[1], true, true, 42);
        assert_eq!(p["time"], json!(ticks(1000).0));
        assert_eq!(p["duration"], json!(ticks(1200).0));
        assert_eq!(p["sourceIn"], json!(0));
        assert_eq!(p["track"], "V1");
        assert_eq!(p["audioTrack"], "A1");
    }

    #[test]
    fn supported_static_gain_and_transform_survive_json_roundtrip() {
        let mut source: Value = serde_json::from_str(SYNTHETIC).unwrap();
        source["master_volume"] = json!(0.5);
        source["clips"][0]["volume"]["default_value"] = json!(0.5);
        source["clips"][0]["pos_x"]["default_value"] = json!(12.0);
        source["clips"][0]["pos_y"]["default_value"] = json!(-8.0);
        source["clips"][0]["scale_x"]["default_value"] = json!(1.25);
        source["clips"][0]["scale_y"]["default_value"] = json!(0.8);
        source["clips"][0]["opacity"]["default_value"] = json!(0.75);
        let encoded = source.to_string();
        let plan = plan_str(&encoded, Path::new("fixtures")).unwrap();
        let reparsed = plan_str(&plan.original.to_string(), Path::new("fixtures")).unwrap();
        let clip = &reparsed.clips[0];
        assert_eq!((clip.volume, clip.pos_x, clip.pos_y, clip.scale_x, clip.scale_y, clip.opacity), (0.5, 12.0, -8.0, 1.25, 0.8, 0.75));
        assert!((reparsed.master_gain_db + 6.020_599_913_279_624).abs() < 1e-9);
    }

    #[test]
    fn unsupported_features_are_reported_before_conversion() {
        let mut source: Value = serde_json::from_str(SYNTHETIC).unwrap();
        source["titles"] = json!([{"text":"Unsupported title"}]);
        source["clips"][0]["effects"] = json!([{"name":"unknown"}]);
        source["clips"][0]["brightness"]["keyframes"] = json!([{"time_ms":0,"value":1.2,"mode":"Linear"}]);
        source["clips"][1]["start"] = json!(1001);
        let report = plan_str(&source.to_string(), Path::new("fixtures")).unwrap_err().to_string();
        for term in ["titles", "effects", "brightness", "off the 60 fps frame grid"] {
            assert!(report.contains(term), "missing {term}: {report}");
        }
    }

    fn write_tone_wav(path: &Path, duration_ms: u32) {
        let samples = duration_ms * 8; // 8 kHz, exactly one sample per 0.125 ms.
        let data_len = samples * 2;
        let mut bytes = Vec::with_capacity(44 + data_len as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&8_000u32.to_le_bytes());
        bytes.extend_from_slice(&16_000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for sample in 0..samples {
            let phase = (sample % 32) as f64 * std::f64::consts::TAU / 32.0;
            let value = (16_384.0 * phase.sin()).round() as i16;
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        std::fs::write(path, bytes).unwrap();
    }

    fn single_clip_source(path: &Path) -> Value {
        let mut source: Value = serde_json::from_str(SYNTHETIC).unwrap();
        source["name"] = json!("single-stream-check");
        source["width"] = json!(2);
        source["height"] = json!(2);
        source["assets"] = json!([{"id":1,"name":"synthetic","uri":path.to_string_lossy(),"duration_ms":1000,"audio_levels":[]}]);
        source["capture_sources"] = json!([]);
        let mut clip = source["clips"][0].clone();
        clip["end"] = json!(1000);
        clip["out_point"] = json!(1000);
        source["clips"] = json!([clip]);
        source
    }

    fn stream_fixture_dir() -> PathBuf {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("clipstrike-stream-test-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_requested_video_refuses_audio_only_media_but_audio_only_request_imports() {
        let dir = stream_fixture_dir();
        let wav = dir.join("audio-only.wav");
        write_tone_wav(&wav, 1000);
        let mut source = single_clip_source(&wav);
        let report = convert(plan_str(&source.to_string(), &dir).unwrap()).err().unwrap().to_string();
        assert!(report.contains("clip 1 requests video") && report.contains("asset 1 has no video stream"), "{report}");
        source["clips"][0]["video_enabled"] = json!(false);
        let session = convert(plan_str(&source.to_string(), &dir).unwrap()).unwrap();
        let seq = session.active_sequence().unwrap();
        assert_eq!(seq.video_tracks.iter().map(|t| t.items.len()).sum::<usize>(), 0);
        assert_eq!(seq.audio_tracks.iter().map(|t| t.items.len()).sum::<usize>(), 1);
    }

    #[test]
    fn missing_requested_audio_refuses_video_only_media_but_video_only_request_imports() {
        let dir = stream_fixture_dir();
        let png = dir.join("video-only.png");
        let pixels = vec![128_u8, 32, 16, 255].repeat(4);
        std::fs::write(&png, filmcraft_export::encode_png(pixels, 2, 2).unwrap()).unwrap();
        let mut source = single_clip_source(&png);
        let report = convert(plan_str(&source.to_string(), &dir).unwrap()).err().unwrap().to_string();
        assert!(report.contains("clip 1 requests audio") && report.contains("asset 1 has no audio stream"), "{report}");
        source["clips"][0]["audio_enabled"] = json!(false);
        let session = convert(plan_str(&source.to_string(), &dir).unwrap()).unwrap();
        let seq = session.active_sequence().unwrap();
        assert_eq!(seq.video_tracks.iter().map(|t| t.items.len()).sum::<usize>(), 1);
        assert_eq!(seq.audio_tracks.iter().map(|t| t.items.len()).sum::<usize>(), 0);
    }

    fn wav_rms_at(path: &Path, seconds: usize) -> f64 {
        let wav = std::fs::read(path).unwrap();
        assert!(wav.starts_with(b"RIFF") && &wav[8..12] == b"WAVE");
        let (mut at, mut format, mut data) = (12usize, None, None);
        while at + 8 <= wav.len() {
            let len = u32::from_le_bytes(wav[at + 4..at + 8].try_into().unwrap()) as usize;
            let start = at + 8;
            assert!(start + len <= wav.len(), "truncated WAV chunk");
            if &wav[at..at + 4] == b"fmt " {
                assert!(len >= 16);
                format = Some((
                    u16::from_le_bytes(wav[start..start + 2].try_into().unwrap()),
                    u16::from_le_bytes(wav[start + 2..start + 4].try_into().unwrap()) as usize,
                    u32::from_le_bytes(wav[start + 4..start + 8].try_into().unwrap()) as usize,
                    u16::from_le_bytes(wav[start + 14..start + 16].try_into().unwrap()),
                ));
            } else if &wav[at..at + 4] == b"data" {
                data = Some(start);
            }
            at = start + len + (len & 1);
        }
        let (codec, channels, sample_rate, bits) = format.unwrap();
        assert_eq!((codec, bits), (1, 16), "expected exported PCM16 WAV");
        let begin = data.unwrap() + seconds * sample_rate * channels * 2;
        let end = begin + (sample_rate / 10) * channels * 2;
        assert!(end <= wav.len());
        let samples: Vec<f64> = wav[begin..end].chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f64).collect();
        (samples.iter().map(|x| x * x).sum::<f64>() / samples.len() as f64).sqrt()
    }

    #[test]
    fn audio_timeline_saves_and_reopens_with_cuts_and_gain() {
        let mut source: Value = serde_json::from_str(SYNTHETIC).unwrap();
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("clipstrike-legacy-test-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (i, asset) in source["assets"].as_array_mut().unwrap().iter_mut().enumerate() {
            let path = dir.join(format!("capture-{i}.wav"));
            write_tone_wav(&path, asset["duration_ms"].as_u64().unwrap() as u32);
            asset["uri"] = json!(path.to_string_lossy().to_string());
        }
        for clip in source["clips"].as_array_mut().unwrap() {
            clip["video_enabled"] = json!(false);
        }
        source["capture_sources"] = json!([]); // The authored audio media is not an HLAE video capture.
        source["clips"][0]["volume"]["default_value"] = json!(0.5);
        let plan = plan_str(&source.to_string(), &dir).unwrap();
        let mut session = convert(plan).unwrap();
        let seq = session.active_sequence().unwrap();
        let mut items: Vec<_> = seq.audio_tracks.iter().flat_map(|t| &t.items).collect();
        items.sort_by_key(|item| item.start);
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].start, items[0].duration, items[0].source_in), (ticks(0), ticks(1000), ticks(0)));
        assert_eq!((items[1].start, items[1].duration, items[1].source_in), (ticks(1000), ticks(1200), ticks(0)));
        assert!((items[0].gain_db + 6.020_599_913_279_624).abs() < 1e-8);
        let first_clip = items[0].id;
        session.execute("clip.audioGain", json!({"clips": [first_clip.0], "mode": "set", "db": -12.0})).unwrap();
        let project = dir.join("converted.fcproj");
        session.execute("file.saveAs", json!({"path": project.to_string_lossy()})).unwrap();
        let mut reopened = Session::default();
        reopened.execute("file.open", json!({"path": project.to_string_lossy()})).unwrap();
        let seq = reopened.active_sequence().unwrap();
        let mut reopened_items: Vec<_> = seq.audio_tracks.iter().flat_map(|t| &t.items).collect();
        reopened_items.sort_by_key(|item| item.start);
        assert_eq!(reopened_items.len(), 2);
        assert_eq!(reopened_items[1].start, ticks(1000));
        assert_eq!(reopened_items[1].duration, ticks(1200));
        assert_eq!(reopened_items[0].gain_db, -12.0);
        assert!(reopened.project.notes.contains("ClipStrike legacy source JSON"));
        let rendered = dir.join("mixed.wav");
        reopened.execute("file.exportMedia", json!({"preset": "Waveform Audio 48 kHz 16-bit", "path": rendered.to_string_lossy(), "wait": true})).unwrap();
        let first = wav_rms_at(&rendered, 0);
        let second = wav_rms_at(&rendered, 1);
        assert!(second > 1_000.0, "exported soundtrack is silent: RMS={second}");
        let ratio = first / second;
        assert!((ratio - 10.0f64.powf(-12.0 / 20.0)).abs() < 0.03, "saved -12 dB clip gain was not applied to exported PCM: ratio={ratio}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
