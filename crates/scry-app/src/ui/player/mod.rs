//! Native video surface and playback controls.
pub(crate) mod actions;
mod interaction;
mod session;
pub(super) mod shortcuts;
pub(crate) mod state;
pub(super) mod stats;
pub(super) mod subtitles;
use super::{
    Page, State,
    controls::action,
    design::{
        icons::{AUDIO, BACK, EXIT_FULLSCREEN, FORWARD, FULLSCREEN, MUTED, PAUSE, PLAY, VOLUME},
        tokens::{panel, theme, type_, video},
    },
    formatting::clock,
    primitives::{
        button::{Button, Variant},
        progress::slider,
        text::label,
    },
};
use actions::PlayerAction;
use rune::{El, Icon, Phase, col, el, row, text};
use scry_app::platform::VideoHost;

fn surface(s: &State, scale: f32, compact: bool) -> El {
    let mut surface = el()
        .key(if compact { "guide-video" } else { "video" })
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(video())
        .on_pointer_move(|s: &mut State, p| {
            s.player.interaction.pointer(
                Some([f64::from(p.x), f64::from(p.y)]),
                std::time::Instant::now(),
            );
        })
        .on_drag(|s: &mut State, drag, tx| {
            if drag.phase == Phase::Up && drag.dx.abs() <= 4.0 && drag.dy.abs() <= 4.0 {
                let click = scry_app::platform::VideoClick {
                    at: std::time::Instant::now(),
                    position: [f64::from(drag.x), f64::from(drag.y)],
                };
                if let Some(action) = s.player.interaction.click(click, s.player.fullscreen) {
                    if matches!(action, PlayerAction::ToggleFullscreen) && s.player.fullscreen {
                        s.set_fullscreen(false);
                    } else {
                        s.player_action(action);
                    }
                    tx.instant();
                }
                tx.wake_after(s.player.interaction.double_click);
            }
        });
    if s.player.playing {
        surface = surface.on_layout(move |s: &mut State, rect| {
            s.player.interaction.video_rect = Some(rect);
            s.player.interaction.scale = scale;
            if s.player.playing && matches!(s.navigation.page, Page::Player | Page::Guide) {
                s.host.place(rect, scale);
                s.cover_player();
                if s.player.ready {
                    s.host.show();
                }
            }
        });
    }
    if s.navigation.page == Page::Guide && !s.player.ready {
        if let Some(image) = s
            .guide_artwork()
            .first()
            .and_then(|url| s.artwork.as_ref()?.get(url))
        {
            surface = surface.child(
                el().absolute(0.0, 0.0)
                    .w_full()
                    .h_full()
                    .pixels_fit(&image, rune::SurfaceFit::Fit),
            );
        }
        surface = surface.child(label(super::guide::preview_status(s)).padding(12.0));
    } else if s.player.playing && !s.player.ready {
        surface = surface.child(loading(s));
    } else if !s.player.playing {
        surface = surface.child(label("Choose another source to continue."));
    }
    surface
}

pub(super) fn player(s: &State, scale: f32) -> El {
    let t = theme();
    let compact = s.navigation.page == Page::Guide && !s.player.fullscreen;
    let surface = surface(s, scale, compact);
    if compact {
        return col()
            .key("guide-preview")
            .min_h(0.0)
            .gap(6.0)
            .child(surface)
            .child(super::guide::player_actions(s, true));
    }
    if s.player.fullscreen {
        let mut page = el()
            .key("player-page")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .child(surface.absolute(0.0, 0.0).h_full())
            .on_hover_change(|s: &mut State, inside, tx| {
                if !inside
                    && !matches!(
                        s.host.pointer_position(s.player.interaction.scale),
                        scry_app::platform::VideoPointer::Inside(_)
                    )
                {
                    s.player
                        .interaction
                        .pointer(None, std::time::Instant::now());
                    tx.instant();
                }
            })
            .on_pointer_move(|s: &mut State, p| {
                s.player.interaction.pointer(
                    Some([f64::from(p.x), f64::from(p.y)]),
                    std::time::Instant::now(),
                );
            });
        if s.player.interaction.visible {
            page = page.child(controls(s).pin(Some(0.0), None, Some(0.0), Some(0.0)));
        }
        return page;
    }
    col()
        .key("player-page")
        .grow(1.0)
        .gap(12.0)
        .child(
            row()
                .center_y()
                .gap(16.0)
                .child(
                    text(s.player.title.clone())
                        .truncate()
                        .font(type_::PLAYER_TITLE)
                        .color(t.text)
                        .grow(1.0),
                )
                .child(action("player-back", "Sources", |s| s.travel(false))),
        )
        .child(surface)
        .child(controls(s))
}

fn loading(s: &State) -> El {
    let image = |url: &Option<String>| {
        url.as_ref()
            .and_then(|url| s.player_artwork.as_ref()?.get(url))
    };
    let mut loading = el()
        .key("player-loading")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full();
    if let Some(background) = image(&s.player.background) {
        loading = loading.child(
            el().key("player-loading-background")
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .pixels_fit(&background, rune::SurfaceFit::Fill)
                .opacity(super::design::tokens::opacity::PLAYER_BACKDROP)
                .transition(rune::Transition::opacity()),
        );
    }
    let title = if let Some(logo) = image(&s.player.logo) {
        el().key("player-loading-logo")
            .w(360.0)
            .max_w(480.0)
            .h(120.0)
            .pixels_fit(&logo, rune::SurfaceFit::Fit)
            .transition(rune::Transition::opacity())
    } else {
        text(s.player.title.clone())
            .key("player-loading-title")
            .font(type_::LOADING_TITLE)
            .color(theme().text)
    };
    loading = loading.child(
        col()
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .center()
            .gap(24.0)
            .padding(24.0)
            .child(title)
            .child(
                label(if s.player.sample.buffering {
                    "Buffering…"
                } else {
                    "Loading…"
                })
                .key("player-loading-status"),
            ),
    );
    if s.player.fullscreen {
        loading = loading
            .child(action("player-back", "Sources", |s| s.travel(false)).absolute(16.0, 16.0));
    }
    loading
}

fn timeline(s: &State) -> El {
    let progress = match (
        s.player.seek_preview.or(s.player.sample.position_s),
        s.player.sample.duration_s,
    ) {
        (Some(position), Some(duration)) if duration.is_finite() && duration > 0.0 => {
            (position / duration).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };
    keyboard_control(super::primitives::progress::buffered_slider(
        "player-timeline",
        progress,
        buffered_range(s),
    ))
    .w_full()
    .label("Seek through video")
    .on_drag(|s: &mut State, drag, _| {
        s.player.interaction.dragging = drag.phase != Phase::Up;
        s.player.interaction.reveal(std::time::Instant::now());
        if !s.player.playing || drag.w <= 0.0 {
            return;
        }
        let Some(duration) = s
            .player
            .sample
            .duration_s
            .filter(|n| n.is_finite() && *n > 0.0)
        else {
            return;
        };
        let target = f64::from((drag.x / drag.w).clamp(0.0, 1.0)) * duration;
        if drag.phase == Phase::Up {
            s.player_action(PlayerAction::SeekTo(target));
            s.player.seek_preview = None;
        } else {
            s.player.seek_preview = Some(target);
        }
    })
}

fn controls(s: &State) -> El {
    let position = s.player.seek_preview.or(s.player.sample.position_s);
    let buttons = row()
        .gap(4.0)
        .center_y()
        .child(icon_button(
            s,
            "pause",
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
            "seek-back",
            "Back 10 seconds (J)",
            &BACK,
            |s| {
                s.player_action(PlayerAction::SeekRelative(-10));
            },
        ))
        .child(icon_button(
            s,
            "seek-forward",
            "Forward 10 seconds (L)",
            &FORWARD,
            |s| {
                s.player_action(PlayerAction::SeekRelative(10));
            },
        ))
        .child(icon_button(
            s,
            "mute",
            if s.player.muted {
                "Unmute (M)"
            } else {
                "Mute (M)"
            },
            if s.player.muted || s.player.volume == 0 {
                &MUTED
            } else {
                &VOLUME
            },
            |s| s.player_action(PlayerAction::ToggleMute),
        ))
        .child(
            keyboard_control(slider(
                "player-volume",
                if s.player.muted {
                    0.0
                } else {
                    f64::from(s.player.volume) / 100.0
                },
            ))
            .w(64.0)
            .label(format!("Volume {}% (↑ / ↓)", s.player.volume))
            .on_drag(|s: &mut State, drag, _| {
                s.player.interaction.dragging = drag.phase != Phase::Up;
                s.player.interaction.reveal(std::time::Instant::now());
                if s.player.playing && drag.w > 0.0 {
                    s.player_action(PlayerAction::SetVolume(volume_at(drag.x / drag.w)));
                }
            }),
        )
        .child(
            label(format!(
                "{} / {}",
                clock(position),
                clock(s.player.sample.duration_s)
            ))
            .key("player-time"),
        )
        .child(el().grow(1.0))
        .child(subtitles::trigger(s))
        .child(icon_button(s, "audio", "Cycle audio track", &AUDIO, |s| {
            s.player_action(PlayerAction::CycleAudio);
        }))
        .child(stats::trigger(s))
        .child(fullscreen_button(s));
    control_panel(s, buttons)
}

fn control_panel(s: &State, buttons: El) -> El {
    let mut controls = col()
        .key("player-controls")
        .w_full()
        .padding(12.0)
        .gap(6.0)
        .radius(8.0)
        .fill(panel())
        .child(timeline(s))
        .child(buttons)
        .on_layout(|s: &mut State, rect| {
            s.player.interaction.controls_rect = Some(rect);
            s.cover_player();
        })
        .on_pointer_move(|s: &mut State, p| {
            s.player.interaction.pointer(
                Some([f64::from(p.x), f64::from(p.y)]),
                std::time::Instant::now(),
            );
        });
    if s.navigation.page == Page::Guide {
        controls = controls.child(super::guide::player_actions(s, false));
    }
    if s.player.fullscreen {
        controls
    } else {
        controls.child(label("Space / K  play / pause     ← / →  seek 5s     J / L  seek 10s     ↑ / ↓  volume     M  mute"))
    }
}

fn buffered_range(s: &State) -> Option<(f64, f64)> {
    if !s.player.playing || s.player.seek_preview.is_some() {
        return None;
    }
    let position = s.player.sample.position_s?;
    let duration = s.player.sample.duration_s?;
    let cached = s.player.sample.cache_s?;
    if !position.is_finite()
        || !duration.is_finite()
        || duration <= 0.0
        || !cached.is_finite()
        || cached <= 0.0
    {
        return None;
    }
    let start = (position / duration).clamp(0.0, 1.0);
    let end = ((position + cached) / duration).clamp(start, 1.0);
    (end > start).then_some((start, end))
}

fn fullscreen_button(s: &State) -> El {
    keyboard_control(
        Button::new(
            "fullscreen",
            if s.player.fullscreen {
                "Exit full screen (F / Esc)"
            } else {
                "Full screen (F)"
            },
        )
        .variant(Variant::Ghost)
        .icon(if s.player.fullscreen {
            &EXIT_FULLSCREEN
        } else {
            &FULLSCREEN
        })
        .enabled(s.player.playing || s.player.fullscreen)
        .on_press_tx(|s: &mut State, tx| {
            s.set_fullscreen(!s.player.fullscreen);
            tx.instant();
        }),
    )
}

fn icon_button(
    state: &State,
    key: &'static str,
    title: &'static str,
    glyph: &'static Icon,
    f: impl Fn(&mut State) + 'static,
) -> El {
    keyboard_control(
        Button::new(key, title)
            .variant(Variant::Ghost)
            .icon(glyph)
            .enabled(state.player.playing)
            .on_press_tx(move |s: &mut State, tx| {
                if s.player.playing {
                    let fullscreen = s.player.fullscreen;
                    f(s);
                    if s.player.fullscreen != fullscreen {
                        tx.instant();
                    }
                }
            }),
    )
}

pub(super) fn keyboard_control(control: El) -> El {
    control.on_physical_input(|s: &mut State, event, tx| {
        if s.player.fullscreen && matches!(event, rune::PhysicalInput::Key { pressed: true, .. }) {
            s.player.interaction.keyboard = true;
            s.player.interaction.reveal(std::time::Instant::now());
            tx.instant();
        }
        false
    })
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "A clamped volume percentage fits in u32"
)]
fn volume_at(fraction: f32) -> u32 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() as u32
}
