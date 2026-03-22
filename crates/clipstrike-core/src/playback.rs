use crate::timeline::Clip;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackStep {
    pub playhead_ms: i64,
    pub should_stop: bool,
}

pub fn advance_playhead(current_ms: i64, delta_ms: i64, timeline_end_ms: i64) -> PlaybackStep {
    let timeline_end_ms = timeline_end_ms.max(0);
    let next_playhead_ms = current_ms
        .saturating_add(delta_ms.max(0))
        .clamp(0, timeline_end_ms);

    PlaybackStep {
        playhead_ms: next_playhead_ms,
        should_stop: next_playhead_ms >= timeline_end_ms,
    }
}

pub fn clip_eos_step(clips: &[Clip], clip_id: usize, timeline_end_ms: i64) -> Option<PlaybackStep> {
    let clip = clips.iter().find(|clip| clip.id == clip_id)?;
    let boundary_ms = clip.end.clamp(0, timeline_end_ms.max(0));
    let should_stop = !clips.iter().any(|candidate| {
        candidate.id != clip_id && candidate.start <= boundary_ms && boundary_ms < candidate.end
    });

    Some(PlaybackStep {
        playhead_ms: boundary_ms,
        should_stop,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::Clip;

    #[test]
    fn advance_playhead_clamps_to_timeline_end() {
        let step = advance_playhead(4_950, 100, 5_000);

        assert_eq!(
            step,
            PlaybackStep {
                playhead_ms: 5_000,
                should_stop: true,
            }
        );
    }

    #[test]
    fn clip_eos_step_stops_when_no_clip_continues_at_boundary() {
        let clips = vec![Clip::new(1, 10, 1, 0, 5_000, 0, 5_000)];

        let step = clip_eos_step(&clips, 1, 5_000).expect("clip should exist");

        assert_eq!(
            step,
            PlaybackStep {
                playhead_ms: 5_000,
                should_stop: true,
            }
        );
    }

    #[test]
    fn clip_eos_step_keeps_playing_when_next_clip_starts_at_boundary() {
        let clips = vec![
            Clip::new(1, 10, 1, 0, 5_000, 0, 5_000),
            Clip::new(2, 11, 1, 5_000, 8_000, 0, 3_000),
        ];

        let step = clip_eos_step(&clips, 1, 8_000).expect("clip should exist");

        assert_eq!(
            step,
            PlaybackStep {
                playhead_ms: 5_000,
                should_stop: false,
            }
        );
    }
}
