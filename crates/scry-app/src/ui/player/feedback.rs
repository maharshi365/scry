//! Playback toggles use mpv OSD; buffering reveals the shared loading surface.
use super::super::{Page, State, design::tokens};
use scry_app::platform::VideoHost;
use std::time::{Duration, Instant};

const OVERLAY_ID: &str = "1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cue {
    Play,
    Pause,
    Buffering,
}

#[derive(Default)]
pub(crate) struct Feedback {
    toggle: Option<(Cue, Instant)>,
    buffering_since: Option<Instant>,
    seeking_since: Option<Instant>,
    drawn: bool,
    loading: bool,
    loading_since: Option<Instant>,
}

impl Feedback {
    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn loading_elapsed(&self) -> Duration {
        self.loading_since
            .map_or(Duration::ZERO, |start| start.elapsed())
    }

    fn playback_frame(
        &mut self,
        sample: &scry_playback::telemetry::Sample,
        now: Instant,
    ) -> Option<(Cue, Duration)> {
        let seeking = if sample.seeking && !sample.paused {
            let start = *self.seeking_since.get_or_insert(now);
            now.saturating_duration_since(start) >= tokens::motion::SEEK_FEEDBACK_DELAY
        } else {
            self.seeking_since = None;
            false
        };
        self.frame(sample.buffering || seeking, sample.paused, now)
    }

    pub fn toggle(&mut self, paused: bool, now: Instant) {
        self.toggle = Some((if paused { Cue::Pause } else { Cue::Play }, now));
    }

    fn frame(&mut self, buffering: bool, paused: bool, now: Instant) -> Option<(Cue, Duration)> {
        if let Some((cue, start)) = self.toggle {
            let elapsed = now.saturating_duration_since(start);
            if elapsed < tokens::motion::PLAYBACK_FEEDBACK {
                return Some((cue, elapsed));
            }
            self.toggle = None;
        }
        if buffering && !paused {
            let start = *self.buffering_since.get_or_insert(now);
            Some((Cue::Buffering, now.saturating_duration_since(start)))
        } else {
            self.buffering_since = None;
            None
        }
    }
}

impl State {
    pub(super) fn clear_player_feedback(&mut self) {
        if self.player.interaction.feedback.drawn {
            self.command(&["osd-overlay", OVERLAY_ID, "none", ""]);
        }
        self.player.interaction.feedback = Feedback::default();
    }

    pub(super) fn poll_player_feedback(&mut self, tx: &mut rune::Tx) -> bool {
        let visible =
            self.player.playing && matches!(self.navigation.page, Page::Player | Page::Guide);
        if !visible {
            let changed = self.player.interaction.feedback.loading;
            self.clear_player_feedback();
            return changed;
        }
        let now = Instant::now();
        if !self.player.ready {
            self.player
                .interaction
                .feedback
                .loading_since
                .get_or_insert(now);
            tx.wake_after(tokens::motion::PLAYBACK_FRAME);
            return true;
        }
        let frame = self
            .player
            .interaction
            .feedback
            .playback_frame(&self.player.sample, now);
        let loading = matches!(frame, Some((Cue::Buffering, _)));
        let changed = self.player.interaction.feedback.loading != loading;
        self.player.interaction.feedback.loading = loading;
        if loading {
            self.player
                .interaction
                .feedback
                .loading_since
                .get_or_insert(now);
            if changed {
                self.shell.host.hide();
            }
        } else {
            self.player.interaction.feedback.loading_since = None;
            if changed {
                self.shell.host.show();
            }
        }
        if loading {
            if self.player.interaction.feedback.drawn {
                self.command(&["osd-overlay", OVERLAY_ID, "none", ""]);
                self.player.interaction.feedback.drawn = false;
            }
            tx.wake_after(tokens::motion::PLAYBACK_FRAME);
            return true;
        }
        let Some((cue, elapsed)) = frame else {
            if self.player.interaction.feedback.drawn {
                self.command(&["osd-overlay", OVERLAY_ID, "none", ""]);
                self.player.interaction.feedback.drawn = false;
            }
            if let Some(start) = self.player.interaction.feedback.seeking_since {
                tx.wake_after(
                    tokens::motion::SEEK_FEEDBACK_DELAY
                        .saturating_sub(now.saturating_duration_since(start)),
                );
            }
            return changed;
        };
        let Some([_, _, width, height]) = self.player.interaction.video_rect else {
            return changed;
        };
        if width <= 0.0 || height <= 0.0 {
            return changed;
        }
        let resolution_x = tokens::player::OSD_HEIGHT * width / height;
        let data = super::super::primitives::feedback::video_cue(cue.into(), elapsed, resolution_x);
        self.command(&[
            "osd-overlay",
            OVERLAY_ID,
            "ass-events",
            &data,
            &format!("{resolution_x:.0}"),
            &format!("{:.0}", tokens::player::OSD_HEIGHT),
        ]);
        self.player.interaction.feedback.drawn = true;
        tx.wake_after(tokens::motion::PLAYBACK_FRAME);
        changed
    }
}

impl From<Cue> for super::super::primitives::feedback::VideoCue {
    fn from(cue: Cue) -> Self {
        match cue {
            Cue::Play => Self::Play,
            Cue::Pause => Self::Pause,
            Cue::Buffering => Self::Buffering,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_feedback_follows_current_buffering_metrics_and_player_visibility() {
        let mut state = crate::ui::app::init();
        state.navigation.page = Page::Player;
        state.player.playing = true;
        state.player.interaction.video_rect = Some([0.0, 0.0, 1280.0, 720.0]);
        state.player.sample.buffering = true;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(
            !state.player.interaction.feedback.drawn,
            "Startup uses the loading surface"
        );
        state.player.ready = true;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(
            state.player.interaction.feedback.loading,
            "A ready player must show buffering feedback"
        );
        state.player.sample.paused = true;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(
            !state.player.interaction.feedback.loading,
            "Pausing clears buffering feedback"
        );
        state.player.sample.paused = false;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(state.player.interaction.feedback.loading);
        state.player.sample.buffering = false;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(
            !state.player.interaction.feedback.loading,
            "Recovery clears buffering feedback"
        );
        state.player.sample.buffering = true;
        state.poll_player_feedback(&mut rune::Tx::default());
        state.navigation.page = Page::Library;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(
            !state.player.interaction.feedback.loading,
            "Leaving the player clears native feedback"
        );
    }

    #[test]
    fn seek_feedback_waits_then_covers_cache_handoff_and_recovery() {
        let now = Instant::now();
        let delay = Duration::from_millis(200);
        let mut feedback = Feedback::default();
        let mut sample = scry_playback::telemetry::Sample {
            seeking: true,
            ..Default::default()
        };
        assert!(feedback.playback_frame(&sample, now).is_none());
        assert!(feedback.playback_frame(&sample, now + delay / 2).is_none());
        assert!(matches!(
            feedback.playback_frame(&sample, now + delay),
            Some((Cue::Buffering, _))
        ));
        sample.seeking = false;
        sample.buffering = true;
        assert!(matches!(
            feedback.playback_frame(&sample, now + delay),
            Some((Cue::Buffering, _))
        ));
        sample.buffering = false;
        assert!(feedback.playback_frame(&sample, now + delay).is_none());
    }

    #[test]
    fn quick_seeks_and_paused_seeks_do_not_show_feedback() {
        let now = Instant::now();
        let mut feedback = Feedback::default();
        let mut sample = scry_playback::telemetry::Sample {
            seeking: true,
            ..Default::default()
        };
        assert!(feedback.playback_frame(&sample, now).is_none());
        sample.seeking = false;
        assert!(
            feedback
                .playback_frame(&sample, now + Duration::from_millis(100))
                .is_none()
        );
        sample.seeking = true;
        assert!(
            feedback
                .playback_frame(&sample, now + Duration::from_secs(1))
                .is_none(),
            "A new seek must start its own delay"
        );
        sample.paused = true;
        assert!(
            feedback
                .playback_frame(&sample, now + Duration::from_secs(2))
                .is_none()
        );
        sample.paused = false;
        assert!(
            feedback
                .playback_frame(&sample, now + Duration::from_secs(3))
                .is_none(),
            "Unpausing must not reuse an old seek timer"
        );
    }

    #[test]
    fn polling_preserves_seek_delay_until_feedback_is_drawn() {
        let mut state = crate::ui::app::init();
        state.navigation.page = Page::Player;
        state.player.playing = true;
        state.player.ready = true;
        state.player.interaction.video_rect = Some([0.0, 0.0, 1280.0, 720.0]);
        state.player.sample.seeking = true;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(!state.player.interaction.feedback.drawn);
        assert!(state.player.interaction.feedback.seeking_since.is_some());
        state.player.interaction.feedback.seeking_since =
            Instant::now().checked_sub(Duration::from_secs(1));
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(state.player.interaction.feedback.loading);
        state.navigation.page = Page::Library;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(!state.player.interaction.feedback.loading);
        assert!(state.player.interaction.feedback.seeking_since.is_none());
    }

    #[test]
    fn toggles_restart_feedback_and_expire() {
        let now = Instant::now();
        let mut feedback = Feedback::default();
        feedback.toggle(true, now);
        assert_eq!(
            feedback.frame(false, true, now),
            Some((Cue::Pause, Duration::ZERO))
        );
        let later = now + Duration::from_millis(200);
        feedback.toggle(false, later);
        assert_eq!(
            feedback.frame(false, false, later),
            Some((Cue::Play, Duration::ZERO))
        );
        assert!(
            feedback
                .frame(false, false, later + tokens::motion::PLAYBACK_FEEDBACK)
                .is_none()
        );
    }

    #[test]
    fn buffering_clears_on_resume_or_pause() {
        let now = Instant::now();
        let mut feedback = Feedback::default();
        assert!(feedback.frame(false, false, now).is_none());
        assert_eq!(
            feedback.frame(true, false, now),
            Some((Cue::Buffering, Duration::ZERO))
        );
        assert!(
            feedback.frame(true, true, now).is_none(),
            "A user pause is not buffering"
        );
        assert_eq!(
            feedback.frame(true, false, now),
            Some((Cue::Buffering, Duration::ZERO))
        );
        assert!(
            feedback.frame(false, false, now).is_none(),
            "Playback recovery removes the spinner"
        );
    }

    #[test]
    fn pause_feedback_takes_priority_over_buffering_then_loading_returns_on_play() {
        let now = Instant::now();
        let mut feedback = Feedback::default();
        feedback.toggle(true, now);
        assert_eq!(
            feedback.frame(true, true, now),
            Some((Cue::Pause, Duration::ZERO))
        );
        assert!(
            feedback
                .frame(true, true, now + tokens::motion::PLAYBACK_FEEDBACK)
                .is_none()
        );
        feedback.toggle(false, now);
        assert_eq!(
            feedback.frame(true, false, now),
            Some((Cue::Play, Duration::ZERO))
        );
        assert!(matches!(
            feedback.frame(true, false, now + tokens::motion::PLAYBACK_FEEDBACK),
            Some((Cue::Buffering, _))
        ));
    }
}
