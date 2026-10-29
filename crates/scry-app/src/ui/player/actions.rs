//! Player actions are shared by keyboard bindings, buttons, and other features.
use super::super::State;
use scry_app::platform::VideoHost;

#[derive(Clone, Copy)]
pub(crate) enum PlayerAction {
    TogglePause,
    ToggleMute,
    SeekRelative(i32),
    SeekTo(f64),
    SetVolume(u32),
    ChangeVolume(i32),
    ToggleSubtitles,
    ShowSubtitles,
    SubtitleDelay(i32),
    SubtitleSize(i32),
    ToggleFullscreen,
}

impl State {
    pub(crate) fn player_action(&mut self, action: PlayerAction) {
        if !self.player.playing {
            return;
        }
        match action {
            PlayerAction::TogglePause => {
                self.player.sample.paused = !self.player.sample.paused;
                self.command(&[
                    "set",
                    "pause",
                    if self.player.sample.paused {
                        "yes"
                    } else {
                        "no"
                    },
                ]);
            }
            PlayerAction::ToggleMute => {
                self.player.muted = !self.player.muted;
                self.command(&["set", "mute", if self.player.muted { "yes" } else { "no" }]);
            }
            PlayerAction::SeekRelative(seconds) => {
                self.player.sample.cache_s = None;
                self.library.observed = None;
                self.command(&["seek", &seconds.to_string(), "relative"]);
            }
            PlayerAction::SeekTo(target) => {
                if !target.is_finite() {
                    return;
                }
                let target = self
                    .player
                    .sample
                    .duration_s
                    .filter(|duration| duration.is_finite() && *duration > 0.0)
                    .map_or_else(|| target.max(0.0), |duration| target.clamp(0.0, duration));
                self.player.sample.cache_s = None;
                self.library.observed = None;
                self.player.sample.position_s = Some(target);
                self.command(&["seek", &target.to_string(), "absolute"]);
            }
            PlayerAction::ChangeVolume(delta) => {
                self.player_action(PlayerAction::SetVolume(
                    self.player.volume.saturating_add_signed(delta),
                ));
            }
            PlayerAction::SetVolume(volume) => {
                self.player.volume = volume.min(100);
                self.player.muted = false;
                self.command(&["set", "mute", "no"]);
                self.command(&["set", "volume", &self.player.volume.to_string()]);
            }
            PlayerAction::ToggleSubtitles => self.toggle_subtitles(),
            PlayerAction::ShowSubtitles => self.show_subtitles(),
            PlayerAction::SubtitleDelay(direction) => self.adjust_subtitle_delay(direction),
            PlayerAction::SubtitleSize(direction) => self.adjust_subtitle_size(direction),
            PlayerAction::ToggleFullscreen => self.set_fullscreen(!self.player.fullscreen),
        }
    }

    pub(in crate::ui) fn set_fullscreen(&mut self, fullscreen: bool) {
        if self.player.fullscreen == fullscreen {
            return;
        }
        match self.host.set_fullscreen(fullscreen || self.app_fullscreen) {
            Ok(()) => {
                self.player.fullscreen = fullscreen;
                self.host.cover(0.0, None, self.player.interaction.scale);
                self.player.stats_overlay.dismiss();
                self.player.interaction.reset();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(in crate::ui) fn toggle_app_fullscreen(&mut self) {
        let fullscreen = !self.app_fullscreen;
        match self
            .host
            .set_fullscreen(fullscreen || self.player.fullscreen)
        {
            Ok(()) => self.app_fullscreen = fullscreen,
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_seek_rejects_nonfinite_targets_without_changing_progress() {
        let mut state = crate::ui::app::init();
        state.player.playing = true;
        state.player.sample.position_s = Some(12.0);
        state.player.sample.cache_s = Some(4.0);
        for target in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            state.player_action(PlayerAction::SeekTo(target));
            assert_eq!(state.player.sample.position_s, Some(12.0));
            assert_eq!(state.player.sample.cache_s, Some(4.0));
        }
    }

    #[test]
    fn absolute_seek_stays_within_the_media_duration() {
        let mut state = crate::ui::app::init();
        state.player.playing = true;
        state.player.sample.duration_s = Some(60.0);
        for (target, expected) in [(-5.0, 0.0), (70.0, 60.0), (25.0, 25.0)] {
            state.player_action(PlayerAction::SeekTo(target));
            assert_eq!(state.player.sample.position_s, Some(expected));
        }
    }
}
