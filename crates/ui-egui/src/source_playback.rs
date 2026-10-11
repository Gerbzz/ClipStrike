//! Source-file auditioning. Playback is presentation state: it never edits the project.

use std::sync::{Arc, Mutex};

use filmcraft_engine::clip_ops::source_view;
use filmcraft_project::ItemId;
use filmcraft_render::SourceProvider;
use filmcraft_time::Tick;
use serde_json::json;

use crate::{AUDIO_STALL_S, FilmcraftApp, Playback};

#[derive(Default)]
pub struct SourcePlayback {
    pub clock: Playback,
    item: Option<ItemId>,
    shown: Tick,
    range_start: Option<Tick>,
    failure: Arc<Mutex<Option<String>>>,
}

impl FilmcraftApp {
    pub fn toggle_source_play(&mut self) -> Result<(), String> {
        if self.source_playback.clock.playing {
            self.stop_source();
            Ok(())
        } else {
            self.play_source()
        }
    }

    pub fn play_source(&mut self) -> Result<(), String> {
        self.start_source_playback(None, None, 1.0)
    }

    /// J / K / L in the Source monitor: play at `speed` × real time (negative = backward) from the
    /// Source playhead. Sound plays at normal forward speed only, as in the Program monitor.
    pub fn shuttle_source(&mut self, speed: f64) -> Result<(), String> {
        if !speed.is_finite() || speed == 0.0 {
            return Err("Source shuttle speed must be a non-zero number".into());
        }
        self.start_source_playback(None, None, speed.clamp(-8.0, 8.0))
    }

    pub fn play_source_range(&mut self, from_playhead: bool, preroll: bool) -> Result<(), String> {
        let item = self.session.state.source_item.ok_or("Open a Source clip first")?;
        let view = source_view(&self.session, item).ok_or("Source item is unavailable")?;
        let range = view.selected_range();
        if range.duration.0 <= 0 {
            return Err("Source marked range is empty".into());
        }
        let seconds = |v: f64| Tick::from_seconds_f64(if v.is_finite() { v.clamp(0.0, 3600.0) } else { 0.0 });
        let start = if from_playhead {
            self.session.state.source_playhead
        } else if preroll {
            Tick(range.start.0.saturating_sub(seconds(self.session.prefs.playback.preroll_seconds).0)).max(view.start)
        } else {
            range.start
        };
        let stop =
            if preroll { Tick(range.end().0.saturating_add(seconds(self.session.prefs.playback.postroll_seconds).0)).min(view.end) } else { range.end() };
        if start >= stop {
            return Err("Source playhead is at or beyond the Out point".into());
        }
        self.start_source_playback(Some(start), Some(stop), 1.0)
    }

    fn start_source_playback(&mut self, start: Option<Tick>, stop: Option<Tick>, speed: f64) -> Result<(), String> {
        let item = self.session.state.source_item.ok_or("Open a media file in the Source monitor first")?;
        let view = source_view(&self.session, item).ok_or("Source item is unavailable")?;
        let media = self.session.project.item(view.media).and_then(|i| i.as_media()).ok_or("Source playback currently supports media files and subclips")?;
        if media.offline {
            return Err("Source media is offline: use Link Media first".into());
        }
        if view.end <= view.start {
            return Err("Source clip has no playable duration".into());
        }
        let looping = self.source_playback.clock.looping;
        let range = view.selected_range();
        if looping && range.duration.0 <= 0 {
            return Err("Source marked range is empty".into());
        }
        let first = if looping { range.start } else { view.start };
        let end = stop.unwrap_or(if looping { range.end() } else { view.end });
        self.stop();
        self.stop_source();
        let mut time = start.unwrap_or(self.session.state.source_playhead);
        // forward from the end (or before a looped range) starts over; backward runs from where it is
        if start.is_none() && speed > 0.0 && (time < first || time >= view.rate.snap(Tick(end.0.saturating_sub(view.rate.frame_duration().0))).max(first)) {
            time = first;
        }
        self.session.execute("source.setPlayhead", json!({"time": time.0})).map_err(|e| e.to_string())?;
        let time = self.session.state.source_playhead;
        self.source_playback.range_start = start;
        self.source_playback.item = Some(item);
        self.source_playback.shown = time;
        self.source_playback.clock = Playback { playing: true, speed, anchor_time: -1.0, anchor_tick: time, looping, stop_at: stop, ..Default::default() };
        self.source_playback.clock.meter.start(speed);
        self.start_source_audio();
        Ok(())
    }

    pub fn stop_source(&mut self) {
        if !self.source_playback.clock.playing {
            return;
        }
        self.source_playback.clock.playing = false;
        self.source_playback.clock.audio_clock = false;
        self.source_playback.clock.preroll = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.source_playback.clock.audio_preparing = false;
            self.source_playback.clock.audio_activated = false;
        }
        self.source_playback.clock.meter.finish();
        self.frames.stop_prefetch();
        if let Some(audio) = self.audio.as_mut() {
            audio.stop();
        }
    }

    fn start_source_audio(&mut self) {
        self.source_playback.clock.audio_clock = false;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.source_playback.clock.audio_preparing = false;
            self.source_playback.clock.audio_activated = false;
            self.source_playback.clock.preroll = None;
        }
        if (self.source_playback.clock.speed - 1.0).abs() > 1e-9 {
            if let Some(audio) = self.audio.as_mut() {
                audio.stop();
            }
            return;
        }
        let Some(item) = self.source_playback.item else { return };
        let Some(view) = source_view(&self.session, item) else { return };
        let Some(media) = self.session.project.item(view.media).and_then(|i| i.as_media()) else { return };
        if !media.info.has_audio() {
            return;
        }
        let project = self.session.project.clone();
        let provider = self.session.media.provider(project, self.session.services.clone());
        let Some(source) = provider.source(view.media) else {
            self.ui.status = "Source audio is unavailable: check Link Media".into();
            return;
        };
        let Some(audio) = self.audio.as_mut() else { return };
        let sr = audio.sample_rate();
        if sr == 0 || audio.channels() == 0 {
            self.ui.status = "Source audio output has an invalid sample rate or channel count".into();
            return;
        }
        let map = [self.session.prefs.audio_hardware.map_left, self.session.prefs.audio_hardware.map_right];
        let mixdown = filmcraft_audio_dsp::channels::Mixdown::from_id(&self.session.prefs.audio.mixdown_type).unwrap_or_default();
        let channels = media.interpret.audio_channels.as_ref().and_then(|m| m.clips.first()).cloned().unwrap_or_default();
        // A seek gets its own failure slot: a late callback from the old stream cannot stop it.
        self.source_playback.failure = Arc::new(Mutex::new(None));
        let failure = self.source_playback.failure.clone();
        let end = self
            .source_playback
            .clock
            .stop_at
            .unwrap_or_else(|| if self.source_playback.clock.looping { view.selected_range().end() } else { view.end })
            .to_units_floor(i64::from(sr));
        let mix = move |cursor: i64, buf: &mut [f32], ch: usize| {
            buf.fill(0.0);
            if ch == 0 || cursor >= end {
                return;
            }
            let frames = (buf.len() / ch).min(usize::try_from(end.saturating_sub(cursor)).unwrap_or(0));
            match source.audio(cursor, frames, sr) {
                Ok(mut sound) => {
                    if !channels.is_empty() {
                        sound.channels = channels.iter().filter_map(|c| sound.channels.get(usize::from(*c)).cloned()).collect();
                    }
                    let sound = filmcraft_render::audio::to_layout(sound, filmcraft_audio_dsp::channels::Layout::Stereo, mixdown);
                    let Some(left) = sound.channels.first() else { return };
                    let right = sound.channels.get(1).unwrap_or(left);
                    filmcraft_engine::settings::map_output(left, right, buf, ch, map);
                }
                Err(e) => {
                    *failure.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(format!("Source audio decoding failed: {e}"));
                }
            }
        };
        audio.stop();
        let cursor = self.session.state.source_playhead.to_units_floor(i64::from(sr));
        #[cfg(not(target_arch = "wasm32"))]
        let factory = {
            let channels = audio.channels();
            let stats = self.source_playback.clock.audio_stats.clone();
            Box::new(move || crate::play_ahead::spawn(mix, cursor, sr, channels, stats))
        };
        #[cfg(target_arch = "wasm32")]
        let fill = {
            let (mix, mut cursor) = (mix, cursor);
            Box::new(move |buf: &mut [f32], ch: usize| {
                mix(cursor, buf, ch);
                cursor = cursor.saturating_add(i64::try_from(buf.len() / ch.max(1)).unwrap_or(0));
            })
        };
        #[cfg(not(target_arch = "wasm32"))]
        let result = audio.prepare(factory);
        #[cfg(target_arch = "wasm32")]
        let result = audio.start(fill).map(|_| ());
        match result {
            Ok(()) => {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.source_playback.clock.audio_preparing = true;
                    self.source_playback.clock.preroll = Some(-1.0);
                    self.source_playback.clock.anchor_time = -1.0;
                }
                #[cfg(target_arch = "wasm32")]
                {
                    self.source_playback.clock.audio_clock = true;
                }
            }
            Err(e) => self.ui.status = format!("Source audio output failed: {e}"),
        }
    }

    pub(crate) fn advance_source_playback(&mut self, ctx: &egui::Context) {
        if !self.source_playback.clock.playing {
            return;
        }
        let item = self.source_playback.item;
        let Some(view) = item.filter(|i| Some(*i) == self.session.state.source_item).and_then(|i| source_view(&self.session, i)) else {
            self.stop_source();
            return;
        };
        let error = self.source_playback.failure.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        if let Some(error) = error {
            self.stop_source();
            self.ui.status = error;
            return;
        }
        let now = ctx.input(|i| i.time);
        let time = self.session.state.source_playhead;
        if time != self.source_playback.shown {
            self.source_playback.clock.anchor_tick = time;
            self.source_playback.clock.anchor_time = -1.0;
            self.source_playback.clock.audio_seen = (0, now);
            self.source_playback.clock.audio_smoother.reset();
            self.start_source_audio();
            self.source_playback.shown = time;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(since) = self.source_playback.clock.preroll {
            let since = if since < 0.0 { now } else { since };
            self.source_playback.clock.preroll = Some(since);
            let mut initial_audio = None;
            if self.source_playback.clock.audio_activated {
                let first = self.audio.as_ref().and_then(|audio| audio.played_frames().map(|frames| (frames, audio.sample_rate())));
                if let Some((frames, rate)) = first.filter(|(frames, rate)| *frames > 0 && *rate > 0) {
                    initial_audio = Some((frames, rate));
                } else if now - since < 2.0 {
                    ctx.request_repaint();
                    return;
                } else {
                    if let Some(audio) = self.audio.as_mut() {
                        audio.stop();
                    }
                    self.ui.status = "Source audio output timed out: playing without sound".into();
                }
            } else {
                let result = self.audio.as_mut().map(|audio| audio.prepared()).unwrap_or(Err("audio output disappeared".into()));
                match result {
                    Ok(true) => {
                        let activated = self.audio.as_mut().map(|audio| audio.activate_prepared()).unwrap_or(Err("audio output disappeared".into()));
                        match activated {
                            Ok(()) => {
                                self.source_playback.clock.audio_activated = true;
                                ctx.request_repaint();
                                return;
                            }
                            Err(error) => {
                                if let Some(audio) = self.audio.as_mut() {
                                    audio.stop();
                                }
                                self.ui.status = format!("Source audio output failed: {error}");
                            }
                        }
                    }
                    Ok(false) if now - since < 2.0 => {
                        ctx.request_repaint();
                        return;
                    }
                    result => {
                        if let Some(audio) = self.audio.as_mut() {
                            audio.stop();
                        }
                        self.ui.status = match result {
                            Err(error) => format!("Source audio output failed: {error}"),
                            Ok(false) => "Source audio output timed out: playing without sound".into(),
                            Ok(true) => String::new(),
                        };
                    }
                }
            }
            self.source_playback.clock.audio_preparing = false;
            self.source_playback.clock.audio_activated = false;
            self.source_playback.clock.preroll = None;
            self.source_playback.clock.anchor_tick = time;
            self.source_playback.clock.anchor_time = now - initial_audio.map_or(0.0, |(frames, rate)| frames as f64 / f64::from(rate.max(1)));
            self.source_playback.clock.audio_seen = (initial_audio.map_or(0, |(frames, _)| frames), now);
            self.source_playback.clock.audio_clock = initial_audio.is_some();
            if initial_audio.is_some()
                && let Some(note) = self.audio.as_ref().and_then(|audio| audio.note())
            {
                self.ui.status = note;
            }
            self.source_playback.clock.audio_smoother.reset();
        }
        let clock = &mut self.source_playback.clock;
        if clock.anchor_time < 0.0 {
            clock.anchor_time = now;
            clock.audio_seen = (0, now);
            clock.audio_smoother.reset();
        }
        let reading = if clock.audio_clock { self.audio.as_ref().and_then(|a| a.played_frames().map(|n| (n, a.sample_rate()))) } else { None };
        if let Some((frames, _)) = reading
            && frames != clock.audio_seen.0
        {
            clock.audio_seen = (frames, now);
        }
        if clock.audio_clock && (reading.is_none() || now - clock.audio_seen.1 >= AUDIO_STALL_S) {
            clock.anchor_tick = time;
            clock.anchor_time = now;
            clock.audio_smoother.reset();
            clock.audio_clock = false;
            if let Some(audio) = self.audio.as_mut() {
                audio.stop();
            }
            self.ui.status = "Source audio output stopped responding: playing without sound (check Settings ▸ Audio Hardware)".into();
        }
        let wall_elapsed = (now - clock.anchor_time).max(0.0);
        let elapsed = match reading {
            Some((frames, sr)) if clock.audio_clock && sr > 0 => clock.audio_smoother.elapsed(wall_elapsed, frames as f64 / f64::from(sr)),
            _ => wall_elapsed,
        };
        let next = Tick(clock.anchor_tick.0.saturating_add(Tick::from_seconds_f64(elapsed * clock.speed).0));
        if clock.speed < 0.0 {
            // backward: stops on the first frame (as the Program monitor stops at the start)
            let ended = next <= view.start;
            match self.session.execute("source.setPlayhead", json!({"time": next.max(view.start).0})) {
                Ok(_) => self.source_playback.shown = self.session.state.source_playhead,
                Err(e) => {
                    self.stop_source();
                    self.ui.status = e.to_string();
                    return;
                }
            }
            if ended {
                self.stop_source();
            }
            ctx.request_repaint();
            return;
        }
        let range = view.selected_range();
        let end = clock.stop_at.unwrap_or_else(|| if clock.looping { range.end() } else { view.end }).min(view.end);
        let ended = next >= end;
        if ended && clock.looping {
            let start = self.source_playback.range_start.unwrap_or(range.start).max(view.start);
            if let Err(e) = self.session.execute("source.setPlayhead", json!({"time":start.0})) {
                self.stop_source();
                self.ui.status = e.to_string();
                return;
            }
            let time = self.session.state.source_playhead;
            self.source_playback.shown = time;
            self.source_playback.clock.anchor_tick = time;
            self.source_playback.clock.anchor_time = -1.0;
            self.source_playback.clock.audio_seen = (0, now);
            self.source_playback.clock.audio_smoother.reset();
            self.start_source_audio();
            ctx.request_repaint();
            return;
        }
        let next = if ended { Tick(end.0.saturating_sub(view.rate.frame_duration().0)).max(view.start) } else { next.max(view.start) };
        match self.session.execute("source.setPlayhead", json!({"time": next.0})) {
            Ok(_) => self.source_playback.shown = self.session.state.source_playhead,
            Err(e) => {
                self.stop_source();
                self.ui.status = e.to_string();
                return;
            }
        }
        if ended {
            self.stop_source();
        }
        ctx.request_repaint();
    }
}
