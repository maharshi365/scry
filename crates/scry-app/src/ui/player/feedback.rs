//! Transient feedback is composited by mpv, above video and below app menus.
use super::super::{Page, State, design::tokens};
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
    drawn: bool,
}

impl Feedback {
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

    pub(super) fn poll_player_feedback(&mut self, tx: &mut rune::Tx) {
        let visible = self.player.playing
            && self.player.ready
            && matches!(self.navigation.page, Page::Player | Page::Guide);
        if !visible {
            self.clear_player_feedback();
            return;
        }
        let now = Instant::now();
        let frame = self.player.interaction.feedback.frame(
            self.player.sample.buffering,
            self.player.sample.paused,
            now,
        );
        let Some((cue, elapsed)) = frame else {
            self.clear_player_feedback();
            return;
        };
        let Some([_, _, width, height]) = self.player.interaction.video_rect else {
            return;
        };
        if width <= 0.0 || height <= 0.0 {
            return;
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
            state.player.interaction.feedback.drawn,
            "A ready player must show buffering feedback"
        );
        state.player.sample.paused = true;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(
            !state.player.interaction.feedback.drawn,
            "Pausing clears buffering feedback"
        );
        state.player.sample.paused = false;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(state.player.interaction.feedback.drawn);
        state.player.sample.buffering = false;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(
            !state.player.interaction.feedback.drawn,
            "Recovery clears buffering feedback"
        );
        state.player.sample.buffering = true;
        state.poll_player_feedback(&mut rune::Tx::default());
        state.navigation.page = Page::Library;
        state.poll_player_feedback(&mut rune::Tx::default());
        assert!(
            !state.player.interaction.feedback.drawn,
            "Leaving the player clears native feedback"
        );
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
    fn pause_feedback_takes_priority_over_buffering_then_spinner_returns_on_play() {
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
