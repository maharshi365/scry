//! Shared player gestures and fullscreen control visibility.
use super::{PlayerAction, State};
use scry_app::platform::{VideoClick, VideoHost, VideoPointer};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_secs(3);

#[derive(Default)]
pub(crate) enum VideoPress {
    #[default]
    Click,
    Cancelled,
}

pub(crate) struct Interaction {
    pub feedback: super::feedback::Feedback,
    pub visible: bool,
    pub dragging: bool,
    pub keyboard: bool,
    pub controls_rect: Option<[f32; 4]>,
    pub video_rect: Option<[f32; 4]>,
    pub scale: f32,
    pub video_press: VideoPress,
    pointer: Option<[f64; 2]>,
    last_activity: Instant,
}

impl Default for Interaction {
    fn default() -> Self {
        Self {
            feedback: super::feedback::Feedback::default(),
            visible: true,
            dragging: false,
            keyboard: false,
            controls_rect: None,
            video_rect: None,
            scale: 1.0,
            video_press: VideoPress::default(),
            pointer: None,
            last_activity: Instant::now(),
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
}

fn contains([x, y, w, h]: [f32; 4], [px, py]: [f64; 2]) -> bool {
    px >= f64::from(x) && py >= f64::from(y) && px < f64::from(x + w) && py < f64::from(y + h)
}

impl State {
    pub(in crate::ui) fn native_video_click(&mut self, click: VideoClick, tx: &mut rune::Tx) {
        self.player.interaction.reveal(click.at);
        self.player_action(PlayerAction::TogglePause);
        if click.double {
            self.player_action(PlayerAction::ToggleFullscreen);
        }
        tx.instant();
    }

    pub(in crate::ui) fn cover_player(&self) {
        let interaction = &self.player.interaction;
        let local_overlay = |rect: [f32; 4]| {
            interaction
                .video_rect
                .map(|video| scry_app::platform::VideoOverlay {
                    rect: [rect[0] - video[0], rect[1] - video[1], rect[2], rect[3]],
                    radius: super::super::design::tokens::overlay::RADIUS,
                })
        };
        let controls = (self.player.fullscreen && interaction.visible)
            .then_some(interaction.controls_rect)
            .flatten()
            .and_then(local_overlay);
        let open = if self.player.subtitles.overlay.is_open() {
            &self.player.subtitles.overlay
        } else if self.player.audio.overlay.is_open() {
            &self.player.audio.overlay
        } else {
            &self.player.stats_overlay
        };
        let overlay = if open.is_open() {
            open.rect
                .zip(interaction.video_rect)
                .map(|([x, y, w, h], video)| scry_app::platform::VideoOverlay {
                    rect: [x - video[0], y - video[1], w, h],
                    radius: super::super::design::tokens::overlay::RADIUS,
                })
        } else {
            super::intro::phase(self)
                .and(self.player.intro_rect)
                .and_then(local_overlay)
        };
        self.shell.host.cover(controls, overlay, interaction.scale);
    }

    fn poll_player_overlays(&mut self, active: bool, tx: &mut rune::Tx) -> bool {
        let subtitles_was = self.player.subtitles.overlay.is_open();
        let audio_was = self.player.audio.overlay.is_open();
        let overlay_was = self.player.stats_overlay.is_open();
        if active {
            let now = Instant::now();
            match self
                .shell
                .host
                .pointer_position(self.player.interaction.scale)
            {
                VideoPointer::Inside(point) => {
                    self.player.stats_overlay.pointer(Some(point), now, tx);
                    self.player.subtitles.overlay.pointer(Some(point), now, tx);
                    self.player.audio.overlay.pointer(Some(point), now, tx);
                }
                VideoPointer::Outside => {
                    self.player.stats_overlay.pointer(None, now, tx);
                    self.player.subtitles.overlay.pointer(None, now, tx);
                    self.player.audio.overlay.pointer(None, now, tx);
                }
                VideoPointer::Unavailable => {}
            }
        } else {
            self.player.stats_overlay.dismiss();
            self.player.subtitles.overlay.dismiss();
            self.player.audio.overlay.dismiss();
        }
        let subtitles_changed = self.player.subtitles.overlay.poll(Instant::now());
        let audio_changed = self.player.audio.overlay.poll(Instant::now());
        if self.player.audio.overlay.is_open() && !audio_was {
            self.player.subtitles.overlay.dismiss();
        }
        if self.player.subtitles.overlay.is_open() {
            self.player.stats_overlay.dismiss();
            self.player.audio.overlay.dismiss();
        } else if self.player.audio.overlay.is_open() {
            self.player.stats_overlay.dismiss();
        }
        let overlay_changed = self.player.stats_overlay.poll(Instant::now())
            || overlay_was != self.player.stats_overlay.is_open()
            || subtitles_changed
            || audio_changed
            || audio_was != self.player.audio.overlay.is_open()
            || subtitles_was != self.player.subtitles.overlay.is_open();
        if let Some(services) = &self.runtime.services {
            services.observe_stream_stats(
                (active && self.player.stats_overlay.is_open() && self.player.torrent_source)
                    .then_some(self.player.generation),
            );
        }
        overlay_changed
    }

    pub(in crate::ui) fn poll_player_interaction(&mut self, tx: &mut rune::Tx) -> bool {
        let clicks = self.shell.host.video_clicks();
        let active = self.player.playing
            && (self.navigation.page == super::Page::Player
                || self.navigation.page == super::Page::Guide && self.player.fullscreen);
        let overlay_changed = self.poll_player_overlays(active, tx);
        if !self.player.playing
            || !matches!(
                self.navigation.page,
                super::Page::Player | super::Page::Guide
            )
        {
            self.poll_player_feedback(tx);
            return overlay_changed;
        }
        let now = Instant::now();
        let mut acted = false;
        let was = (self.player.interaction.visible, self.player.fullscreen);
        for click in clicks {
            if self.player.audio.overlay.is_open() {
                self.player.audio.overlay.dismiss();
                acted = true;
                continue;
            }
            if self.player.subtitles.overlay.is_open() {
                self.player.subtitles.overlay.dismiss();
                acted = true;
                continue;
            }
            self.native_video_click(click, tx);
            acted = true;
        }
        self.poll_player_feedback(tx);
        if self.player.fullscreen {
            let interaction = &mut self.player.interaction;
            let pointer = match self.shell.host.pointer_position(interaction.scale) {
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
            if !self.player.stats_overlay.is_open()
                && !self.player.subtitles.overlay.is_open()
                && !self.player.audio.overlay.is_open()
            {
                interaction.idle(now);
            }
        }
        self.cover_player();
        tx.wake_after(Duration::from_millis(50));
        let changed = overlay_changed
            || acted
            || was != (self.player.interaction.visible, self.player.fullscreen);
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
