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
    CycleSubtitles,
    CycleAudio,
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
            PlayerAction::CycleSubtitles => self.command(&["cycle", "sid"]),
            PlayerAction::CycleAudio => self.command(&["cycle", "aid"]),
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
                self.host.cover_bottom(0.0, self.player.interaction.scale);
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
