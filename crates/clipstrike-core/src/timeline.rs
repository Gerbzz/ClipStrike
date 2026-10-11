use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::keyframes::KeyframedProperty;

fn default_clip_speed() -> KeyframedProperty {
    KeyframedProperty::new(1.0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Copy)]
#[repr(u32)]
pub enum BlendMode {
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Overlay = 3,
    Darken = 4,
    Lighten = 5,
    ColorDodge = 6,
    ColorBurn = 7,
    HardLight = 8,
    SoftLight = 9,
    Difference = 10,
    Exclusion = 11,
    Hue = 12,
    Saturation = 13,
    Color = 14,
    Luminosity = 15,
    Add = 16,
    Subtract = 17,
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Normal
    }
}

/// Represents a video clip or timeline element with a start and end time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Clip {
    pub id: usize,
    pub asset_id: usize, // Link to MediaAsset in Project
    pub track_id: usize, // Represents Z-order layering
    pub start: i64,
    pub end: i64,
    pub in_point: i64,
    pub out_point: i64,

    #[serde(default)]
    pub blend_mode: BlendMode,

    // Keyframed Properties (Phase 9)
    pub brightness: KeyframedProperty,
    pub contrast: KeyframedProperty,
    pub saturation: KeyframedProperty,
    pub grayscale: KeyframedProperty,
    pub blur_radius: KeyframedProperty,
    pub volume: KeyframedProperty,
    pub pos_x: KeyframedProperty,
    pub pos_y: KeyframedProperty,
    pub scale_x: KeyframedProperty,
    pub scale_y: KeyframedProperty,
    pub rotation: KeyframedProperty,
    pub opacity: KeyframedProperty,
    #[serde(default = "default_clip_speed")]
    pub speed: KeyframedProperty,
    #[serde(default)]
    pub pixelate_size: KeyframedProperty,
    #[serde(default)]
    pub gamma: KeyframedProperty,
    #[serde(default)]
    pub hue: KeyframedProperty,
    #[serde(default)]
    pub color_tint_r: KeyframedProperty,
    #[serde(default)]
    pub color_tint_g: KeyframedProperty,
    #[serde(default)]
    pub color_tint_b: KeyframedProperty,
    #[serde(default)]
    pub chroma_key_threshold: KeyframedProperty,
    #[serde(default)]
    pub chroma_key_fuzziness: KeyframedProperty,
    #[serde(default)]
    pub chroma_key_r: KeyframedProperty,
    #[serde(default)]
    pub chroma_key_g: KeyframedProperty,
    #[serde(default)]
    pub chroma_key_b: KeyframedProperty,
    #[serde(default)]
    pub lut_enabled: KeyframedProperty,
    #[serde(default)]
    pub lut_path: Option<String>,
    #[serde(default)]
    pub lut_size: u32,
    #[serde(default)]
    pub reverse: bool,
    #[serde(default)]
    pub freeze_frame_ms: Option<i64>,
    #[serde(default = "default_enabled")]
    pub video_enabled: bool,
    #[serde(default = "default_enabled")]
    pub audio_enabled: bool,

    #[serde(default)]
    pub effects: Vec<crate::effects::Effect>,

    // AI & Computer Vision (Phase 21)
    #[serde(default)]
    pub stabilization_enabled: bool,
    #[serde(default)]
    pub motion_tracking_enabled: bool,
    #[serde(default)]
    pub stabilization_strength: KeyframedProperty,
    #[serde(default)]
    pub object_detection_enabled: bool,
    #[serde(default)]
    pub tracking_data: Vec<crate::effects::MotionTrack>,
    #[serde(default)]
    pub detections: Vec<crate::effects::DetectedObject>,

    #[serde(default)]
    pub group_id: Option<usize>,
    #[serde(default)]
    pub color_tag: [u8; 4],
}

fn default_enabled() -> bool {
    true
}

impl Clip {
    pub fn new(
        id: usize,
        asset_id: usize,
        track_id: usize,
        start: i64,
        end: i64,
        in_point: i64,
        out_point: i64,
    ) -> Self {
        Self {
            id,
            asset_id,
            track_id,
            start,
            end,
            in_point,
            out_point,
            blend_mode: BlendMode::Normal,
            brightness: KeyframedProperty::new(1.0),
            contrast: KeyframedProperty::new(1.0),
            saturation: KeyframedProperty::new(1.0),
            grayscale: KeyframedProperty::new(0.0),
            blur_radius: KeyframedProperty::new(0.0),
            volume: KeyframedProperty::new(1.0),
            pos_x: KeyframedProperty::new(0.0),
            pos_y: KeyframedProperty::new(0.0),
            scale_x: KeyframedProperty::new(1.0),
            scale_y: KeyframedProperty::new(1.0),
            rotation: KeyframedProperty::new(0.0),
            opacity: KeyframedProperty::new(1.0),
            speed: KeyframedProperty::new(1.0),
            pixelate_size: KeyframedProperty::new(1.0),
            gamma: KeyframedProperty::new(1.0),
            hue: KeyframedProperty::new(0.0),
            color_tint_r: KeyframedProperty::new(1.0),
            color_tint_g: KeyframedProperty::new(1.0),
            color_tint_b: KeyframedProperty::new(1.0),
            chroma_key_threshold: KeyframedProperty::new(0.0),
            chroma_key_fuzziness: KeyframedProperty::new(0.1),
            chroma_key_r: KeyframedProperty::new(0.0),
            chroma_key_g: KeyframedProperty::new(1.0),
            chroma_key_b: KeyframedProperty::new(0.0),
            lut_enabled: KeyframedProperty::new(0.0),
            lut_path: None,
            lut_size: 0,
            reverse: false,
            freeze_frame_ms: None,
            video_enabled: true,
            audio_enabled: true,
            effects: Vec::new(),
            stabilization_enabled: false,
            motion_tracking_enabled: false,
            stabilization_strength: KeyframedProperty::new(0.0),
            object_detection_enabled: false,
            tracking_data: Vec::new(),
            detections: Vec::new(),
            group_id: None,
            color_tag: [
                rand::random::<u8>(),
                rand::random::<u8>(),
                rand::random::<u8>(),
                255,
            ],
        }
    }

    pub fn duration_ms(&self) -> i64 {
        (self.end - self.start).max(1)
    }

    pub fn speed_multiplier_at(&self, timeline_time_ms: i64) -> f32 {
        self.speed.get_value_at(timeline_time_ms).max(0.01)
    }

    pub fn source_duration_ms(&self) -> i64 {
        if let Some(freeze_frame_ms) = self.freeze_frame_ms {
            return freeze_frame_ms.max(0);
        }
        (self.out_point - self.in_point).max(0)
    }

    pub fn source_time_at(&self, timeline_time_ms: i64) -> i64 {
        if let Some(freeze_frame_ms) = self.freeze_frame_ms {
            return freeze_frame_ms.max(0);
        }

        let offset = self.integrated_source_offset(timeline_time_ms);
        if self.reverse {
            self.out_point.saturating_sub(offset).max(self.in_point)
        } else {
            self.in_point.saturating_add(offset).min(self.out_point)
        }
    }

    fn integrated_source_offset(&self, timeline_time_ms: i64) -> i64 {
        let clamped_end = timeline_time_ms.clamp(self.start, self.end);
        if clamped_end <= self.start {
            return 0;
        }

        let span_ms = clamped_end - self.start;
        // Most clips have a fixed speed. Avoid walking every 16 ms from the
        // start of a long clip and losing precision in absolute f32 times.
        if self.speed.keyframes.is_empty() {
            return (span_ms as f64 * self.speed_multiplier_at(self.start) as f64)
                .round() as i64;
        }
        let steps = ((span_ms + 15) / 16).max(1);
        let dt = span_ms as f32 / steps as f32;
        let mut area = 0.0f32;
        let mut previous_time = self.start as f32;
        let mut previous_speed = self.speed_multiplier_at(self.start);

        for step in 1..=steps {
            let current_time = self.start as f32 + dt * step as f32;
            let current_speed = self.speed_multiplier_at(current_time.round() as i64);
            area += (previous_speed + current_speed) * 0.5 * (current_time - previous_time);
            previous_time = current_time;
            previous_speed = current_speed;
        }

        area.round() as i64
    }

    /// Convert a source media position (in milliseconds) back to a timeline position.
    /// This is the inverse of `source_time_at()` — given where GStreamer currently is
    /// in the source file, compute where the playhead should be on the timeline.
    pub fn timeline_time_from_source(&self, source_ms: i64) -> i64 {
        if self.freeze_frame_ms.is_some() {
            // Freeze-frame clips don't advance source position, so we can't
            // derive timeline position from source position. Return clip start.
            return self.start;
        }

        let source_offset = if self.reverse {
            self.out_point - source_ms.clamp(self.in_point, self.out_point)
        } else {
            source_ms.clamp(self.in_point, self.out_point) - self.in_point
        };

        // For constant speed (the common case), this is an exact inversion:
        // source_offset = speed * (timeline_time - clip.start)
        // => timeline_time = clip.start + source_offset / speed
        let speed = self.speed_multiplier_at(self.start) as f64;
        let timeline_offset = (source_offset as f64 / speed).round() as i64;
        self.start.saturating_add(timeline_offset).clamp(self.start, self.end)
    }
}

#[derive(Debug)]
struct Node {
    clip: Clip,
    max_end: i64,
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
}

impl Node {
    fn new(clip: Clip) -> Self {
        let max_end = clip.end;
        Self {
            clip,
            max_end,
            left: None,
            right: None,
        }
    }
}

/// An Interval Tree to store clips and query overlapping clips efficiently (O(log N)).
pub struct IntervalTree {
    root: Option<Box<Node>>,
}

impl IntervalTree {
    pub fn new() -> Self {
        Self { root: None }
    }

    /// Inserts a new clip into the interval tree.
    pub fn insert(&mut self, clip: Clip) {
        let new_node = Box::new(Node::new(clip));
        if let Some(ref mut root) = self.root {
            Self::insert_node(root, new_node);
        } else {
            self.root = Some(new_node);
        }
    }

    fn insert_node(root: &mut Node, new_node: Box<Node>) {
        if new_node.clip.start < root.clip.start {
            if let Some(ref mut left) = root.left {
                Self::insert_node(left, new_node);
            } else {
                root.left = Some(new_node);
            }
        } else {
            if let Some(ref mut right) = root.right {
                Self::insert_node(right, new_node);
            } else {
                root.right = Some(new_node);
            }
        }

        // Update the max_end of the current node
        root.max_end = root.max_end.max(root.clip.end);
        if let Some(ref left) = root.left {
            root.max_end = root.max_end.max(left.max_end);
        }
        if let Some(ref right) = root.right {
            root.max_end = root.max_end.max(right.max_end);
        }
    }

    /// Queries the tree to find all clips that overlap with the specific timestamp.
    /// Overlap is defined as [start, end) where start <= time < end.
    pub fn query_point(&self, time: i64) -> Vec<Clip> {
        let mut result = Vec::new();
        if let Some(ref root) = self.root {
            Self::query_point_node(root, time, &mut result);
        }
        result
    }

    fn query_point_node(node: &Node, time: i64, result: &mut Vec<Clip>) {
        // If the current node's maximum end time is less than or equal to the time,
        // no intervals in this subtree can overlap.
        if time >= node.max_end {
            return;
        }

        if let Some(ref left) = node.left {
            Self::query_point_node(left, time, result);
        }

        // Check if the current node overlaps the queried time point
        if node.clip.start <= time && time < node.clip.end {
            result.push(node.clip.clone());
        }

        // Only search the right subtree if the queried time is greater than or equal
        // to the current node's start time.
        if time >= node.clip.start {
            if let Some(ref right) = node.right {
                Self::query_point_node(right, time, result);
            }
        }
    }

    /// Queries the tree to find all clips that overlap with the specific time range [start, end).
    pub fn query_range(&self, start: i64, end: i64) -> Vec<Clip> {
        let mut result = Vec::new();
        if let Some(ref root) = self.root {
            Self::query_range_node(root, start, end, &mut result);
        }
        result
    }

    fn query_range_node(node: &Node, start: i64, end: i64, result: &mut Vec<Clip>) {
        if start >= node.max_end {
            return;
        }

        if let Some(ref left) = node.left {
            Self::query_range_node(left, start, end, result);
        }

        // Overlap condition: node.start < end AND node.end > start
        if node.clip.start < end && node.clip.end > start {
            result.push(node.clip.clone());
        }

        if end > node.clip.start {
            if let Some(ref right) = node.right {
                Self::query_range_node(right, start, end, result);
            }
        }
    }
}

/// A Spatial Hash Engine for calculating magnetic attraction between clip edges.
pub struct SnapEngine {
    bucket_size: i64,
    hash_grid: HashMap<i64, Vec<i64>>,
}

impl SnapEngine {
    /// Creates a new snap engine with the given bucket cell size.
    pub fn new(bucket_size: i64) -> Self {
        Self {
            bucket_size,
            hash_grid: HashMap::new(),
        }
    }

    fn get_bucket(&self, time: i64) -> i64 {
        time.div_euclid(self.bucket_size)
    }

    /// Adds the start and end edges of a clip to the spatial hash.
    pub fn add_clip(&mut self, clip: &Clip) {
        self.add_edge(clip.start);
        self.add_edge(clip.end);
    }

    /// Adds a single edge (e.g. a marker or cut) to the spatial hash.
    pub fn add_edge(&mut self, time: i64) {
        let bucket = self.get_bucket(time);
        self.hash_grid
            .entry(bucket)
            .or_insert_with(Vec::new)
            .push(time);
    }

    /// Removes a specific edge from the spatial hash.
    pub fn remove_edge(&mut self, time: i64) {
        let bucket = self.get_bucket(time);
        if let Some(edges) = self.hash_grid.get_mut(&bucket) {
            if let Some(pos) = edges.iter().position(|&t| t == time) {
                edges.remove(pos);
            }
            if edges.is_empty() {
                self.hash_grid.remove(&bucket);
            }
        }
    }

    /// Finds the closest edge within the specified `radius`.
    /// Returns None if no edge is found within the snapping radius.
    pub fn find_closest_edge(&self, time: i64, radius: i64) -> Option<i64> {
        self.find_closest_edge_excluding(time, radius, &[])
    }

    pub fn find_closest_edge_excluding(
        &self,
        time: i64,
        radius: i64,
        excluded_edges: &[i64],
    ) -> Option<i64> {
        let min_time = time - radius;
        let max_time = time + radius;

        let start_bucket = self.get_bucket(min_time);
        let end_bucket = self.get_bucket(max_time);

        let mut closest_edge = None;
        let mut min_dist = radius + 1;

        for b in start_bucket..=end_bucket {
            if let Some(edges) = self.hash_grid.get(&b) {
                for &edge in edges {
                    if excluded_edges.contains(&edge) {
                        continue;
                    }
                    let dist = (edge - time).abs();
                    if dist <= radius && dist < min_dist {
                        min_dist = dist;
                        closest_edge = Some(edge);
                    }
                }
            }
        }

        closest_edge
    }
}

use crate::transitions::Transition;

/// The main timeline manager, holding all tracks, clips, and snapping logic.
pub struct Timeline {
    pub clips: Vec<Clip>,
    pub transitions: Vec<Transition>,
    pub tree: IntervalTree,
    pub snap_engine: SnapEngine,
}

impl Timeline {
    pub fn new() -> Self {
        Self {
            clips: Vec::new(),
            transitions: Vec::new(),
            tree: IntervalTree::new(),
            snap_engine: SnapEngine::new(1000), // 1 second snap buckets at 1ms precision
        }
    }

    pub fn from_parts(clips: Vec<Clip>, transitions: Vec<Transition>) -> Self {
        let mut timeline = Self {
            clips,
            transitions,
            tree: IntervalTree::new(),
            snap_engine: SnapEngine::new(1000),
        };
        timeline.rebuild_indices();
        timeline
    }

    pub fn rebuild_indices(&mut self) {
        self.snap_engine = SnapEngine::new(1000);
        for clip in &self.clips {
            self.snap_engine.add_clip(clip);
        }
        self.rebuild_tree();
    }

    pub fn preview_move(
        &self,
        old_start: i64,
        old_end: i64,
        delta_ms: i64,
        snap_radius_ms: i64,
        excluded_edges: &[i64],
    ) -> (i64, i64) {
        let duration_ms = (old_end - old_start).max(1);
        let mut new_start = old_start.saturating_add(delta_ms).max(0);
        let mut new_end = new_start.saturating_add(duration_ms);

        let start_snap =
            self.snap_engine
                .find_closest_edge_excluding(new_start, snap_radius_ms, excluded_edges);
        let end_snap =
            self.snap_engine
                .find_closest_edge_excluding(new_end, snap_radius_ms, excluded_edges);

        let snapped_edge = [
            start_snap.map(|edge| (true, edge)),
            end_snap.map(|edge| (false, edge)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(is_start, edge)| {
            let probe = if *is_start { new_start } else { new_end };
            (edge - probe).abs()
        });

        if let Some((is_start, edge)) = snapped_edge {
            if is_start {
                new_start = edge.max(0);
                new_end = new_start.saturating_add(duration_ms);
            } else {
                new_start = edge.saturating_sub(duration_ms).max(0);
                new_end = new_start.saturating_add(duration_ms);
            }
        }

        (new_start, new_end)
    }

    pub fn preview_trim(
        &self,
        old_start: i64,
        old_end: i64,
        trim_left: bool,
        delta_ms: i64,
        snap_radius_ms: i64,
        excluded_edges: &[i64],
    ) -> (i64, i64) {
        let mut new_start = old_start;
        let mut new_end = old_end;

        if trim_left {
            new_start = old_start.saturating_add(delta_ms).clamp(0, old_end - 1);
            if let Some(snapped) = self.snap_engine.find_closest_edge_excluding(
                new_start,
                snap_radius_ms,
                excluded_edges,
            ) {
                new_start = snapped.clamp(0, new_end - 1);
            }
        } else {
            new_end = old_end.saturating_add(delta_ms).max(old_start + 1);
            if let Some(snapped) = self.snap_engine.find_closest_edge_excluding(
                new_end,
                snap_radius_ms,
                excluded_edges,
            ) {
                new_end = snapped.max(new_start + 1);
            }
        }

        (new_start, new_end)
    }

    pub fn add_clip(&mut self, clip: Clip) {
        self.snap_engine.add_clip(&clip);
        self.tree.insert(clip.clone());
        self.clips.push(clip);
        self.check_for_transitions();
    }

    pub fn remove_clip(&mut self, id: usize) {
        if let Some(pos) = self.clips.iter().position(|c| c.id == id) {
            let clip = self.clips.remove(pos);
            self.snap_engine.remove_edge(clip.start);
            self.snap_engine.remove_edge(clip.end);
            self.rebuild_tree();
            self.check_for_transitions();
        }
    }

    /// Restore an edited clip without changing its timeline order.
    pub fn replace_clip(&mut self, clip: Clip) -> Result<(), String> {
        let pos = self.clips.iter().position(|current| current.id == clip.id)
            .ok_or_else(|| "Clip not found".to_string())?;
        let old = &self.clips[pos];
        self.snap_engine.remove_edge(old.start);
        self.snap_engine.remove_edge(old.end);
        self.snap_engine.add_edge(clip.start);
        self.snap_engine.add_edge(clip.end);
        self.clips[pos] = clip;
        self.rebuild_tree();
        self.check_for_transitions();
        Ok(())
    }

    /// Keep clip stacking order stable when an edit temporarily removes a clip.
    pub fn move_clip_to_index(&mut self, id: usize, index: usize) -> Result<(), String> {
        let pos = self.clips.iter().position(|clip| clip.id == id)
            .ok_or_else(|| "Clip not found".to_string())?;
        let clip = self.clips.remove(pos);
        self.clips.insert(index.min(self.clips.len()), clip);
        self.rebuild_tree();
        Ok(())
    }

    pub fn rebuild_tree(&mut self) {
        let mut new_tree = IntervalTree::new();
        for clip in &self.clips {
            new_tree.insert(clip.clone());
        }
        self.tree = new_tree;
    }

    pub fn move_clip(&mut self, id: usize, new_start: i64, new_end: i64) -> Result<(), String> {
        if let Some(pos) = self.clips.iter().position(|c| c.id == id) {
            let old_clip = self.clips[pos].clone();
            self.snap_engine.remove_edge(old_clip.start);
            self.snap_engine.remove_edge(old_clip.end);

            let sanitized_start = new_start.max(0);
            let sanitized_end = new_end.max(sanitized_start + 1);
            self.clips[pos].start = sanitized_start;
            self.clips[pos].end = sanitized_end;

            self.snap_engine.add_edge(sanitized_start);
            self.snap_engine.add_edge(sanitized_end);

            self.rebuild_tree();
            self.check_for_transitions();
            Ok(())
        } else {
            Err("Clip not found".into())
        }
    }

    pub fn move_clip_to_track(&mut self, id: usize, new_track_id: usize) -> Result<(), String> {
        if let Some(pos) = self.clips.iter().position(|c| c.id == id) {
            self.clips[pos].track_id = new_track_id;
            self.rebuild_tree();
            self.check_for_transitions();
            Ok(())
        } else {
            Err("Clip not found".into())
        }
    }

    /// Splits a clip at the given position, creating two new clips.
    pub fn razor(&mut self, id: usize, split_at: i64) -> Result<(), String> {
        if let Some(pos) = self.clips.iter().position(|c| c.id == id) {
            let clip = self.clips[pos].clone();
            if split_at <= clip.start || split_at >= clip.end {
                return Err("Split point outside clip bounds".into());
            }

            let split_source_at = clip.source_time_at(split_at);
            let mut left = clip.clone();
            left.end = split_at;
            let mut right = clip.clone();
            right.id = loop {
                let candidate = rand::random::<usize>();
                if !self.clips.iter().any(|existing| existing.id == candidate) {
                    break candidate;
                }
            };
            let right_id = right.id;
            right.start = split_at;
            if clip.reverse {
                left.in_point = split_source_at;
                right.out_point = split_source_at;
            } else {
                left.out_point = split_source_at;
                right.in_point = split_source_at;
            }

            self.remove_clip(id);
            self.add_clip(left);
            self.add_clip(right);
            self.move_clip_to_index(id, pos)?;
            self.move_clip_to_index(right_id, pos + 1)?;

            Ok(())
        } else {
            Err("Clip not found".into())
        }
    }

    /// Trims a clip's edges.
    pub fn trim(
        &mut self,
        id: usize,
        new_start: i64,
        new_end: i64,
        max_source_duration_ms: i64,
    ) -> Result<(), String> {
        if let Some(pos) = self.clips.iter().position(|c| c.id == id) {
            let old_clip = self.clips[pos].clone();
            let sanitized_start = new_start.max(0);
            let sanitized_end = new_end.max(sanitized_start + 1);
            // Edges outside the current clip extrapolate the boundary speed so an
            // outward trim can reveal the source footage that was trimmed earlier.
            let source_offset = |time_ms: i64| -> i64 {
                if time_ms < old_clip.start {
                    -((old_clip.start.saturating_sub(time_ms) as f64
                        * old_clip.speed_multiplier_at(old_clip.start) as f64).round() as i64)
                } else if time_ms > old_clip.end {
                    old_clip.integrated_source_offset(old_clip.end).saturating_add(
                        (time_ms.saturating_sub(old_clip.end) as f64
                            * old_clip.speed_multiplier_at(old_clip.end) as f64).round() as i64,
                    )
                } else {
                    old_clip.integrated_source_offset(time_ms)
                }
            };
            let start_offset = source_offset(sanitized_start);
            let end_offset = source_offset(sanitized_end);
            let source_limit = max_source_duration_ms.max(0);
            let (raw_in, raw_out) = if old_clip.reverse {
                (old_clip.out_point.saturating_sub(end_offset),
                 old_clip.out_point.saturating_sub(start_offset))
            } else {
                (old_clip.in_point.saturating_add(start_offset),
                 old_clip.in_point.saturating_add(end_offset))
            };
            let new_in_point = raw_in.clamp(0, source_limit);
            let new_out_point = raw_out.clamp(new_in_point, source_limit);

            self.snap_engine.remove_edge(old_clip.start);
            self.snap_engine.remove_edge(old_clip.end);

            self.clips[pos].start = sanitized_start;
            self.clips[pos].end = sanitized_end;
            self.clips[pos].in_point = new_in_point;
            self.clips[pos].out_point = new_out_point;

            self.snap_engine.add_edge(sanitized_start);
            self.snap_engine.add_edge(sanitized_end);

            self.rebuild_tree();
            self.check_for_transitions();
            Ok(())
        } else {
            Err("Clip not found".into())
        }
    }

    /// Task 11.1: Automatic Transition Detection.
    /// Searches for overlapping clips on the same track and creates crossfades.
    pub fn check_for_transitions(&mut self) {
        let existing_transitions: HashMap<(usize, i64, i64), Transition> = self
            .transitions
            .iter()
            .map(|transition| {
                (
                    (transition.track_id, transition.start_ms, transition.end_ms),
                    transition.clone(),
                )
            })
            .collect();
        self.transitions.clear();

        let mut clips_by_track: HashMap<usize, Vec<Clip>> = HashMap::new();
        for clip in &self.clips {
            clips_by_track
                .entry(clip.track_id)
                .or_default()
                .push(clip.clone());
        }

        for (track_id, mut track_clips) in clips_by_track {
            track_clips.sort_by_key(|clip| clip.start);

            for index in 0..track_clips.len().saturating_sub(1) {
                let current = &track_clips[index];
                let next = &track_clips[index + 1];
                if next.start < current.end {
                    let transition_start = next.start;
                    let transition_end = current.end.min(next.end);
                    if let Some(existing_transition) =
                        existing_transitions.get(&(track_id, transition_start, transition_end))
                    {
                        self.transitions.push(existing_transition.clone());
                    } else {
                        self.transitions.push(Transition::new(
                            rand::random::<usize>(),
                            track_id,
                            transition_start,
                            transition_end,
                            crate::transitions::TransitionType::Crossfade,
                        ));
                    }
                }
            }
        }
    }

    /// Task 3.4: Multi-track layering (Z-order)
    /// Returns clips active at the given time, sorted by track_id (z-order)
    pub fn get_clips_at_time(&self, time: i64) -> Vec<Clip> {
        let mut active_clips: Vec<Clip> = self
            .clips
            .iter()
            .filter(|clip| clip.start <= time && clip.end > time)
            .cloned()
            .collect();
        active_clips.sort_by_key(|clip| (clip.track_id, clip.start, clip.id));
        active_clips
    }

    /// Task 3.2: Timeline Virtualization
    /// Returns only clips that are visible within the given horizontal scroll range.
    pub fn get_visible_clips(&self, view_start: i64, view_end: i64) -> Vec<Clip> {
        let mut visible_clips: Vec<Clip> = self
            .clips
            .iter()
            .filter(|clip| clip.end > view_start && clip.start < view_end)
            .cloned()
            .collect();
        visible_clips.sort_by_key(|clip| (clip.track_id, clip.start, clip.id));
        visible_clips
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframes::InterpolationMode;

    #[test]
    fn test_interval_tree_point_query() {
        let mut tree = IntervalTree::new();
        tree.insert(Clip::new(1, 0, 1, 10, 50, 0, 40));
        tree.insert(Clip::new(2, 0, 1, 20, 40, 0, 20));
        tree.insert(Clip::new(3, 0, 1, 45, 60, 0, 15));
        tree.insert(Clip::new(4, 0, 1, 5, 15, 0, 10));

        let result = tree.query_point(30);
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|c| c.id == 1));
        assert!(result.iter().any(|c| c.id == 2));

        let result2 = tree.query_point(48);
        assert_eq!(result2.len(), 2);
        assert!(result2.iter().any(|c| c.id == 1));
        assert!(result2.iter().any(|c| c.id == 3));

        let result3 = tree.query_point(2);
        assert!(result3.is_empty());

        // Left bound is inclusive, Right bound is exclusive
        let result4 = tree.query_point(10);
        assert_eq!(result4.len(), 2);
        assert!(result4.iter().any(|c| c.id == 1)); // start: 10
        assert!(result4.iter().any(|c| c.id == 4)); // end: 15

        // Right bound is exclusive
        let result5 = tree.query_point(50);
        assert_eq!(result5.len(), 1); // Only clip 3 (45 to 60)
        assert!(result5.iter().any(|c| c.id == 3));
    }

    #[test]
    fn test_snap_engine() {
        let mut engine = SnapEngine::new(100);

        let clip = Clip {
            id: 1,
            asset_id: 0,
            track_id: 1,
            blend_mode: BlendMode::Normal,
            start: 150,
            end: 850,
            in_point: 0,
            out_point: 700,
            brightness: KeyframedProperty::new(1.0),
            contrast: KeyframedProperty::new(1.0),
            saturation: KeyframedProperty::new(1.0),
            grayscale: KeyframedProperty::new(0.0),
            blur_radius: KeyframedProperty::new(0.0),
            volume: KeyframedProperty::new(1.0),
            pos_x: KeyframedProperty::new(0.0),
            pos_y: KeyframedProperty::new(0.0),
            scale_x: KeyframedProperty::new(1.0),
            scale_y: KeyframedProperty::new(1.0),
            rotation: KeyframedProperty::new(0.0),
            opacity: KeyframedProperty::new(1.0),
            speed: KeyframedProperty::new(1.0),
            pixelate_size: KeyframedProperty::new(1.0),
            color_tint_r: KeyframedProperty::new(1.0),
            color_tint_g: KeyframedProperty::new(1.0),
            color_tint_b: KeyframedProperty::new(1.0),
            chroma_key_threshold: KeyframedProperty::new(0.0),
            chroma_key_fuzziness: KeyframedProperty::new(0.1),
            chroma_key_r: KeyframedProperty::new(0.0),
            chroma_key_g: KeyframedProperty::new(1.0),
            chroma_key_b: KeyframedProperty::new(0.0),
            gamma: KeyframedProperty::new(1.0),
            hue: KeyframedProperty::new(0.0),
            lut_enabled: KeyframedProperty::new(0.0),
            lut_path: None,
            lut_size: 0,
            reverse: false,
            freeze_frame_ms: None,
            audio_enabled: true,
            video_enabled: true,
            stabilization_enabled: false,
            motion_tracking_enabled: false,
            stabilization_strength: KeyframedProperty::new(0.5),
            object_detection_enabled: false,
            tracking_data: Vec::new(),
            detections: Vec::new(),
            effects: Vec::new(),
            group_id: None,
            color_tag: [80, 120, 160, 255],
        };
        engine.add_clip(&clip);

        // Within snap radius
        let snap1 = engine.find_closest_edge(145, 10);
        assert_eq!(snap1, Some(150));

        // Outside snap radius
        let snap2 = engine.find_closest_edge(130, 10);
        assert_eq!(snap2, None);

        // Near end edge
        let snap3 = engine.find_closest_edge(855, 10);
        assert_eq!(snap3, Some(850));

        // Adding another edge close by
        engine.add_edge(152);
        let snap4 = engine.find_closest_edge(149, 10);
        // Distance to 150 is 1. Distance to 152 is 3. Closest is 150.
        assert_eq!(snap4, Some(150));

        // Test removing an edge
        engine.remove_edge(150);
        let snap5 = engine.find_closest_edge(149, 10);
        // Now 150 is gone, 152 is closest
        assert_eq!(snap5, Some(152));
    }

    #[test]
    fn preview_move_clamps_to_zero_and_snaps_to_neighbor_edge() {
        let mut timeline = Timeline::new();
        timeline.add_clip(make_clip(1, 1, 100, 300));
        timeline.add_clip(make_clip(2, 1, 500, 700));

        let moved = timeline.preview_move(100, 300, -250, 25, &[100, 300]);
        assert_eq!(moved, (0, 200));

        let snapped = timeline.preview_move(100, 300, 185, 25, &[100, 300]);
        assert_eq!(snapped, (300, 500));
    }

    #[test]
    fn preview_trim_clamps_and_snaps_only_the_dragged_edge() {
        let mut timeline = Timeline::new();
        timeline.add_clip(make_clip(1, 1, 100, 300));
        timeline.add_clip(make_clip(2, 1, 500, 700));

        let trimmed_left = timeline.preview_trim(100, 300, true, -250, 25, &[100, 300]);
        assert_eq!(trimmed_left, (0, 300));

        let trimmed_right = timeline.preview_trim(100, 300, false, 190, 25, &[100, 300]);
        assert_eq!(trimmed_right, (100, 500));
    }

    #[test]
    fn visible_and_active_queries_use_current_clip_bounds() {
        let mut timeline = Timeline::new();
        timeline.add_clip(make_clip(1, 1, 0, 100));
        timeline.add_clip(make_clip(2, 1, 300, 400));

        timeline.clips[1].start = 50;
        timeline.clips[1].end = 150;

        let visible_ids: Vec<usize> = timeline
            .get_visible_clips(0, 160)
            .into_iter()
            .map(|clip| clip.id)
            .collect();
        assert_eq!(visible_ids, vec![1, 2]);

        let active_ids: Vec<usize> = timeline
            .get_clips_at_time(75)
            .into_iter()
            .map(|clip| clip.id)
            .collect();
        assert_eq!(active_ids, vec![1, 2]);
    }

    #[test]
    fn razor_splits_clip_into_two_adjacent_segments() {
        let mut timeline = Timeline::new();
        timeline.add_clip(make_clip(1, 1, 0, 100));

        timeline.razor(1, 40).expect("razor should succeed");

        let mut spans: Vec<(i64, i64)> = timeline
            .clips
            .iter()
            .map(|clip| (clip.start, clip.end))
            .collect();
        spans.sort_unstable();
        assert_eq!(spans, vec![(0, 40), (40, 100)]);
    }

    #[test]
    fn clip_new_initializes_ai_defaults() {
        let clip = Clip::new(7, 42, 3, 10, 20, 0, 10);

        assert_eq!(clip.id, 7);
        assert_eq!(clip.asset_id, 42);
        assert_eq!(clip.track_id, 3);
        assert_eq!(clip.start, 10);
        assert_eq!(clip.end, 20);
        assert!(!clip.stabilization_enabled);
        assert!(!clip.motion_tracking_enabled);
        assert!(!clip.object_detection_enabled);
        assert!(clip.tracking_data.is_empty());
        assert!(clip.detections.is_empty());
    }

    fn make_clip(id: usize, track_id: usize, start: i64, end: i64) -> Clip {
        Clip::new(id, 0, track_id, start, end, 0, end - start)
    }

    #[test]
    fn transition_range_is_clamped_to_actual_overlap() {
        let mut timeline = Timeline::new();
        timeline.add_clip(make_clip(1, 1, 0, 100));
        timeline.add_clip(make_clip(2, 1, 50, 60));

        assert_eq!(timeline.transitions.len(), 1);
        let transition = &timeline.transitions[0];
        assert_eq!(transition.start_ms, 50);
        assert_eq!(transition.end_ms, 60);
        assert_eq!(
            transition.transition_type,
            crate::transitions::TransitionType::Crossfade
        );
    }
    #[test]
    fn existing_transition_type_is_preserved_when_overlap_stays_the_same() {
        let mut timeline = Timeline::new();
        timeline.add_clip(make_clip(1, 1, 0, 100));
        timeline.add_clip(make_clip(2, 1, 50, 120));
        timeline.transitions[0].transition_type = crate::transitions::TransitionType::CustomMask;
        timeline.transitions[0].custom_mask_id = Some(55);
        timeline.transitions[0]
            .speed
            .insert_keyframe(75, 2.5, InterpolationMode::Linear);
        timeline.transitions[0]
            .softness
            .insert_keyframe(75, 0.2, InterpolationMode::Linear);

        timeline.check_for_transitions();

        assert_eq!(timeline.transitions.len(), 1);
        assert_eq!(
            timeline.transitions[0].transition_type,
            crate::transitions::TransitionType::CustomMask
        );
        assert_eq!(timeline.transitions[0].custom_mask_id, Some(55));
        assert!(timeline.transitions[0].speed.keyframe(75).is_some());
        assert!(timeline.transitions[0].softness.keyframe(75).is_some());
    }

    #[test]
    fn test_remove_clip_updates_tree_and_snap() {
        let mut timeline = Timeline::new();
        timeline.add_clip(make_clip(1, 1, 0, 100));
        assert_eq!(timeline.clips.len(), 1);

        let active_before = timeline.get_clips_at_time(50);
        assert_eq!(active_before.len(), 1);

        timeline.remove_clip(1);
        assert_eq!(timeline.clips.len(), 0);

        // Tree should be rebuilt and empty
        let active_after = timeline.get_clips_at_time(50);
        assert_eq!(active_after.len(), 0);

        // Snap engine should be empty
        let snap_edge = timeline
            .snap_engine
            .find_closest_edge_excluding(0, 100, &[]);
        assert!(snap_edge.is_none());
    }

    #[test]
    fn test_move_clip_updates_tree_and_snap() {
        let mut timeline = Timeline::new();
        timeline.add_clip(make_clip(1, 1, 0, 100));

        timeline.move_clip(1, 200, 300).unwrap();

        // 50 should now yield nothing, 250 should yield the clip
        assert_eq!(timeline.get_clips_at_time(50).len(), 0);
        assert_eq!(timeline.get_clips_at_time(250).len(), 1);

        // Snap engine should match 200 and 300
        let snap_start = timeline
            .snap_engine
            .find_closest_edge_excluding(190, 20, &[]);
        assert_eq!(snap_start, Some(200));

        let snap_end = timeline
            .snap_engine
            .find_closest_edge_excluding(310, 20, &[]);
        assert_eq!(snap_end, Some(300));

        // Old edges should not snap
        let snap_old = timeline.snap_engine.find_closest_edge_excluding(0, 50, &[]);
        assert!(snap_old.is_none());
    }

    #[test]
    fn test_trim_clip_updates_tree_and_snap() {
        let mut timeline = Timeline::new();
        timeline.add_clip(make_clip(1, 1, 0, 100));

        timeline.trim(1, 50, 150, 1000).unwrap();

        // queries should reflect the new bounds [50, 150)
        assert_eq!(timeline.get_clips_at_time(25).len(), 0);
        assert_eq!(timeline.get_clips_at_time(100).len(), 1);

        // Snap engine should reflect new bounds
        assert_eq!(
            timeline
                .snap_engine
                .find_closest_edge_excluding(40, 20, &[]),
            Some(50)
        );
        assert_eq!(
            timeline
                .snap_engine
                .find_closest_edge_excluding(160, 20, &[]),
            Some(150)
        );

        // Old edges should be removed
        assert!(
            timeline
                .snap_engine
                .find_closest_edge_excluding(0, 10, &[])
                .is_none()
        );
    }

    #[test]
    fn outward_trim_reveals_earlier_and_later_source_frames() {
        let mut timeline = Timeline::new();
        timeline.add_clip(Clip::new(1, 1, 1, 500, 1500, 700, 1700));
        timeline.trim(1, 300, 1800, 3000).unwrap();
        let clip = &timeline.clips[0];
        assert_eq!((clip.start, clip.end), (300, 1800));
        assert_eq!((clip.in_point, clip.out_point), (500, 2000));
    }

    #[test]
    fn reverse_trim_updates_source_edges_in_reverse_order() {
        let mut timeline = Timeline::new();
        let mut clip = Clip::new(1, 1, 1, 500, 1500, 700, 1700);
        clip.reverse = true;
        timeline.add_clip(clip);
        timeline.trim(1, 700, 1300, 3000).unwrap();
        let clip = &timeline.clips[0];
        assert_eq!((clip.in_point, clip.out_point), (900, 1500));
        assert_eq!(clip.source_time_at(700), 1500);
        assert_eq!(clip.source_time_at(1300), 900);
    }

    #[test]
    fn timeline_time_from_source_round_trips_with_source_time_at() {
        // Clip on timeline at [1000, 6000) with in_point=500, out_point=5500
        let clip = Clip::new(1, 10, 1, 1000, 6000, 500, 5500);

        // At timeline time 1000, source_time = in_point = 500
        assert_eq!(clip.source_time_at(1000), 500);
        assert_eq!(clip.timeline_time_from_source(500), 1000);

        // At timeline time 3500 (midpoint), source_time = 500 + 2500 = 3000
        assert_eq!(clip.source_time_at(3500), 3000);
        assert_eq!(clip.timeline_time_from_source(3000), 3500);

        // At timeline time 6000 (end), source_time = 500 + 5000 = 5500 = out_point
        assert_eq!(clip.source_time_at(6000), 5500);
        assert_eq!(clip.timeline_time_from_source(5500), 6000);
    }

    #[test]
    fn constant_speed_source_lookup_is_exact_at_one_hundred_hours() {
        let start = 100 * 60 * 60 * 1_000;
        let mut clip = Clip::new(1, 1, 1, start, start + 100_000, 50_000, 250_000);
        clip.speed.default_value = 1.5;
        assert_eq!(clip.source_time_at(start + 1), 50_002);
        assert_eq!(clip.source_time_at(start + 35), 50_053);
        assert_eq!(clip.source_time_at(start + 50_000), 125_000);
        assert_eq!(clip.timeline_time_from_source(125_000), start + 50_000);
        clip.reverse = true;
        assert_eq!(clip.source_time_at(start + 50_000), 175_000);
        assert_eq!(clip.timeline_time_from_source(175_000), start + 50_000);
    }

    #[test]
    fn timeline_time_from_source_clamps_to_clip_bounds() {
        let clip = Clip::new(1, 10, 1, 0, 5000, 0, 5000);

        // Source position before in_point clamps to clip start
        assert_eq!(clip.timeline_time_from_source(-100), 0);

        // Source position after out_point clamps to clip end
        assert_eq!(clip.timeline_time_from_source(6000), 5000);
    }
}
