//! Miniplayer: playback continues in the window corner while another page is open.
use super::{
    super::{
        Page, State,
        design::{
            icons::{BACK, CLOSE, EXPAND, FORWARD, PAUSE, PLAY},
            tokens::{layout, player, type_},
        },
        formatting::clock,
        primitives::{
            button::{Button, Variant},
            surface,
            text::label,
        },
    },
    actions::PlayerAction,
    icon_button,
};
use rune::{El, row};
use scry_app::platform::VideoHost;

impl State {
    /// True when a page or the miniplayer shows the video surface.
    pub(in crate::ui) fn video_visible(&self) -> bool {
        self.player.playing
            && (self.player.mini || matches!(self.navigation.page, Page::Player | Page::Guide))
    }

    /// Leave the current page for `next`. Ready playback on the player page
    /// moves to the miniplayer when `keep_playing`; other playback stops.
    pub(in crate::ui) fn leave_player(&mut self, next: Page, keep_playing: bool) {
        let page = self.navigation.page;
        if page == Page::Player
            && keep_playing
            && next != Page::Guide
            && self.player.playing
            && self.player.ready
            && self.player.error.is_none()
        {
            self.set_fullscreen(false);
            self.player.mini = true;
            self.player.seek_preview = None;
            self.player.stats_overlay.dismiss();
            self.player.subtitles.overlay.dismiss();
            self.player.audio.overlay.dismiss();
            // Show the controls once so the new corner player is discoverable.
            self.player.interaction.reveal(std::time::Instant::now());
            // The miniplayer layout places and shows the video again.
            self.shell.host.hide();
        } else if matches!(page, Page::Player | Page::Guide)
            // The Guide owns its own live preview.
            || (self.player.mini && next == Page::Guide)
        {
            self.stop();
        }
    }

    /// Move playback from the player page into the miniplayer.
    pub(in crate::ui) fn open_miniplayer(&mut self) {
        if self.navigation.page != Page::Player {
            return;
        }
        if self.navigation.back.is_empty() {
            self.navigate(Page::Watching);
        } else {
            self.travel(false);
        }
    }

    /// Return miniplayer playback to the full player page.
    pub(in crate::ui) fn restore_miniplayer(&mut self) {
        if !self.player.mini {
            return;
        }
        self.navigate(Page::Player);
        self.player.mini = false;
        self.shell.host.hide();
    }

    /// Stop playback that is shown in the miniplayer.
    pub(in crate::ui) fn close_miniplayer(&mut self) {
        if self.player.mini {
            self.stop();
        }
    }
}

pub(in crate::ui) fn view(s: &State, cx: &rune::Cx<State>) -> Option<El> {
    if !s.player.mini || !s.player.playing {
        return None;
    }
    let motion = if cx.is_reduced_motion() {
        super::super::primitives::motion::LoadingMotion::Reduced
    } else {
        super::super::primitives::motion::LoadingMotion::Animated(
            s.player.interaction.feedback.loading_elapsed(),
        )
    };
    let mut panel = surface::Variant::Card
        .apply(rune::el())
        .key("miniplayer-panel")
        .role("region")
        .label(format!("Miniplayer: {}", s.player.title))
        .w(player::MINI_WIDTH)
        .h(player::MINI_HEIGHT)
        .pin(
            None,
            None,
            Some(player::MINI_INSET),
            Some(player::MINI_INSET),
        )
        .child(
            super::surface(s, cx.env().scale, "mini-video", motion)
                .absolute(0.0, 0.0)
                .h_full(),
        )
        .on_hover_change(|s: &mut State, inside, tx| {
            // The native video takes the pointer on Windows; only hide when it left both.
            if !inside
                && !matches!(
                    s.shell.host.pointer_position(s.player.interaction.scale),
                    scry_app::platform::VideoPointer::Inside(_)
                )
            {
                s.player
                    .interaction
                    .pointer(None, std::time::Instant::now());
                tx.instant();
            }
        });
    if s.player.interaction.visible {
        // Controls float over the video and hide like the fullscreen controls.
        let inset = layout::ACTION_GAP;
        panel = panel.child(
            rune::el()
                .pin(Some(inset), None, Some(inset), Some(inset))
                .child(controls(s)),
        );
    }
    Some(panel)
}

fn controls(s: &State) -> El {
    let position = s.player.seek_preview.or(s.player.sample.position_s);
    surface::Variant::Plain
        .apply(row())
        .key("mini-controls")
        .w_full()
        .padding(layout::COPY_GAP)
        .gap(layout::COPY_GAP)
        .center_y()
        .on_layout_tx(|s: &mut State, rect, tx| {
            // The native video leaves a hole here so the controls stay visible.
            s.player.interaction.controls_rect = Some(rect);
            s.cover_player();
            tx.live_only();
        })
        .on_pointer_move(|s: &mut State, p| {
            s.player.interaction.pointer(
                Some([f64::from(p.x), f64::from(p.y)]),
                std::time::Instant::now(),
            );
        })
        .child(icon_button(
            s,
            "mini-seek-back",
            "Back 10 seconds (J)",
            &BACK,
            |s| s.player_action(PlayerAction::SeekRelative(-10)),
        ))
        .child(icon_button(
            s,
            "mini-pause",
            if s.player.sample.paused {
                "Play (Space / K)"
            } else {
                "Pause (Space / K)"
            },
            if s.player.sample.paused {
                &PLAY
            } else {
                &PAUSE
            },
            |s| s.player_action(PlayerAction::TogglePause),
        ))
        .child(icon_button(
            s,
            "mini-seek-forward",
            "Forward 10 seconds (L)",
            &FORWARD,
            |s| s.player_action(PlayerAction::SeekRelative(10)),
        ))
        .child(
            label(format!(
                "{} / {}",
                clock(position),
                clock(s.player.sample.duration_s)
            ))
            .key("mini-time")
            .font(type_::CAPTION),
        )
        .child(rune::el().grow(1.0))
        .child(
            Button::new("mini-expand", "Expand player (I)")
                .variant(Variant::Ghost)
                .icon(&EXPAND)
                .on_press_tx(|s: &mut State, tx| {
                    s.restore_miniplayer();
                    tx.instant();
                }),
        )
        .child(
            Button::new("mini-close", "Close player")
                .variant(Variant::Ghost)
                .icon(&CLOSE)
                .on_press_tx(|s: &mut State, tx| {
                    s.close_miniplayer();
                    tx.instant();
                }),
        )
}
