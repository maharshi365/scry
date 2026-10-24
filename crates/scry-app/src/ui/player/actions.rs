//! Player actions are shared by keyboard bindings, buttons, and other features.
use super::super::State;

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
    Stop,
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
                self.command(&["seek", &seconds.to_string(), "relative"]);
            }
            PlayerAction::SeekTo(target) => {
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
            PlayerAction::Stop => self.stop(),
        }
    }
}
