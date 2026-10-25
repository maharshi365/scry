//! Shared player gestures and fullscreen control visibility.
use super::{PlayerAction, State};
use scry_app::platform::{VideoClick, VideoHost, VideoPointer};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(3);

pub(crate) struct Interaction {
    pub visible: bool,
    pub dragging: bool,
    pub keyboard: bool,
    pub controls_rect: Option<[f32; 4]>,
    pub video_rect: Option<[f32; 4]>,
    pub scale: f32,
    pointer: Option<[f64; 2]>,
    last_activity: Instant,
    pending: Option<(VideoClick, bool)>,
    pub double_click: Duration,
}

impl Default for Interaction {
    fn default() -> Self {
        Self {
            visible: true,
            dragging: false,
            keyboard: false,
            controls_rect: None,
            video_rect: None,
            scale: 1.0,
            pointer: None,
            last_activity: Instant::now(),
            pending: None,
            double_click: Duration::from_millis(500),
        }
    }
}

impl Interaction {
    pub fn reset(&mut self) {
        // Rune reports layout only when geometry changes. Retuning can replace
        // playback without changing any player rectangles.
        *self = Self {
            controls_rect: self.controls_rect,
            video_rect: self.video_rect,
            scale: self.scale,
            ..Self::default()
        };
    }

    pub fn reveal(&mut self, now: Instant) {
        self.visible = true;
        self.last_activity = now;
    }

    pub fn pointer(&mut self, point: Option<[f64; 2]>, now: Instant) {
        if point != self.pointer {
            self.keyboard = false;
            if point.is_some() {
                self.reveal(now);
            }
        }
        self.pointer = point;
        if point.is_none() && !self.dragging && !self.keyboard {
            self.visible = false;
        }
    }

    fn holds_controls(&self) -> bool {
        self.dragging
            || self.keyboard
            || self
                .pointer
                .is_some_and(|p| self.visible && self.controls_rect.is_some_and(|r| contains(r, p)))
    }

    pub fn idle(&mut self, now: Instant) {
        if !self.holds_controls() && now.saturating_duration_since(self.last_activity) >= IDLE {
            self.visible = false;
        }
    }

    pub fn click(&mut self, click: VideoClick, fullscreen: bool) -> Option<PlayerAction> {
        self.reveal(click.at);
        if let Some((first, _)) = self.pending
            && click.at.saturating_duration_since(first.at) <= self.double_click
            && (click.position[0] - first.position[0]).abs() <= 4.0
            && (click.position[1] - first.position[1]).abs() <= 4.0
        {
            self.pending = None;
            return Some(PlayerAction::ToggleFullscreen);
        }
        let previous = self.pending.replace((click, fullscreen));
        previous
            .filter(|(_, fullscreen)| *fullscreen)
            .map(|_| PlayerAction::TogglePause)
    }

    pub fn finish_click(&mut self, now: Instant) -> Option<PlayerAction> {
        let (click, fullscreen) = self.pending?;
        if now.saturating_duration_since(click.at) < self.double_click {
            return None;
        }
        self.pending = None;
        fullscreen.then_some(PlayerAction::TogglePause)
    }
}

fn contains([x, y, w, h]: [f32; 4], [px, py]: [f64; 2]) -> bool {
    px >= f64::from(x) && py >= f64::from(y) && px < f64::from(x + w) && py < f64::from(y + h)
}

impl State {
    pub(in crate::ui) fn poll_player_interaction(&mut self, tx: &mut rune::Tx) -> bool {
        let clicks = self.host.video_clicks();
        if !self.player.playing
            || !matches!(
                self.navigation.page,
                super::Page::Player | super::Page::Guide
            )
        {
            return false;
        }
        let now = Instant::now();
        let mut acted = false;
        let was = (self.player.interaction.visible, self.player.fullscreen);
        self.player.interaction.double_click = self.host.double_click_interval();
        for click in clicks {
            if let Some(action) = self.player.interaction.click(click, self.player.fullscreen) {
                self.player_action(action);
                acted = true;
            }
        }
        if let Some(action) = self.player.interaction.finish_click(now) {
            self.player_action(action);
            acted = true;
        }
        if self.player.fullscreen {
            let interaction = &mut self.player.interaction;
            let pointer = match self.host.pointer_position(interaction.scale) {
                VideoPointer::Unavailable => None,
                VideoPointer::Outside => Some(None),
                VideoPointer::Inside(point) => Some(Some(point)),
            };
            if let Some(pointer) = pointer {
                interaction.pointer(
                    pointer.filter(|p| interaction.video_rect.is_some_and(|r| contains(r, *p))),
                    now,
                );
            }
            interaction.idle(now);
            let cover = if interaction.visible {
                interaction.controls_rect.map_or(100.0, |r| r[3])
            } else {
                0.0
            };
            self.host.cover_bottom(cover, interaction.scale);
        }
        tx.wake_after(Duration::from_millis(50));
        let changed = acted || was != (self.player.interaction.visible, self.player.fullscreen);
        if changed {
            tx.instant();
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_click_waits_and_double_click_only_toggles_fullscreen() {
        let mut input = Interaction::default();
        let now = Instant::now();
        let click = VideoClick {
            at: now,
            position: [10.0, 20.0],
        };
        assert!(input.click(click, true).is_none());
        assert!(
            input
                .finish_click(now + Duration::from_millis(100))
                .is_none()
        );
        assert!(matches!(
            input.click(
                VideoClick {
                    at: now + Duration::from_millis(200),
                    ..click
                },
                true
            ),
            Some(PlayerAction::ToggleFullscreen)
        ));
        assert!(input.finish_click(now + Duration::from_secs(1)).is_none());
        assert!(
            input
                .click(
                    VideoClick {
                        at: now + Duration::from_secs(2),
                        ..click
                    },
                    true
                )
                .is_none()
        );
        assert!(matches!(
            input.finish_click(now + Duration::from_secs(3)),
            Some(PlayerAction::TogglePause)
        ));
    }

    #[test]
    fn double_click_also_enters_fullscreen_but_windowed_single_click_does_not_pause() {
        let mut input = Interaction::default();
        let now = Instant::now();
        let click = VideoClick {
            at: now,
            position: [10.0, 20.0],
        };
        input.click(click, false);
        assert!(input.finish_click(now + Duration::from_secs(1)).is_none());
        input.click(
            VideoClick {
                at: now + Duration::from_secs(2),
                ..click
            },
            false,
        );
        assert!(matches!(
            input.click(
                VideoClick {
                    at: now + Duration::from_millis(2200),
                    ..click
                },
                false
            ),
            Some(PlayerAction::ToggleFullscreen)
        ));
    }

    #[test]
    fn controls_hide_on_leave_or_idle_and_remain_during_interaction() {
        let mut input = Interaction::default();
        let now = Instant::now();
        input.pointer(Some([10.0, 20.0]), now);
        input.idle(now + IDLE);
        assert!(!input.visible);
        input.pointer(Some([11.0, 20.0]), now + IDLE);
        assert!(input.visible);
        input.controls_rect = Some([0.0, 0.0, 100.0, 100.0]);
        input.idle(now + IDLE * 2);
        assert!(input.visible);
        input.pointer(None, now + IDLE * 2);
        assert!(!input.visible);
        input.dragging = true;
        input.reveal(now);
        input.idle(now + IDLE * 3);
        assert!(input.visible);
        input.dragging = false;
        input.keyboard = true;
        input.idle(now + IDLE * 4);
        assert!(input.visible);
    }
}
