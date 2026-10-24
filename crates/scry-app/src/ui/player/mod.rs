//! Native video surface and playback controls.
pub(crate) mod actions;
mod session;
pub(super) mod shortcuts;
pub(crate) mod state;
use super::{
    Page, State,
    controls::action,
    design::{
        icons::{
            AUDIO, BACK, CAPTIONS, EXIT_FULLSCREEN, FORWARD, FULLSCREEN, MUTED, PAUSE, PLAY, STOP,
            VOLUME,
        },
        tokens::{accent, edge, panel, theme, type_, video},
    },
    formatting::{clock, metric},
    primitives::{
        button::{Button, Variant},
        text::label,
    },
};
use actions::PlayerAction;
use rune::{El, Icon, Phase, col, el, row, text};
use scry_app::platform::VideoHost;

pub(super) fn player(s: &State, scale: f32) -> El {
    let t = theme();
    let mut surface = el()
        .key("video")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(video());
    if s.player.playing {
        surface = surface.on_layout(move |s: &mut State, rect| {
            if s.player.playing && s.navigation.page == Page::Player {
                s.host.place(rect, scale);
                if s.player.ready {
                    s.host.show();
                }
            }
        });
        if !s.player.ready {
            surface = surface.child(loading(s));
        }
    } else {
        surface = surface.child(label("Choose another source to continue."));
    }
    if s.player.fullscreen {
        return col()
            .key("player-page")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .child(surface)
            .child(controls(s));
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
        .child(label(format!(
            "cache {}s   ·   drops {}   ·   RAM {} MiB   ·   {}",
            metric(s.player.sample.cache_s),
            s.player
                .sample
                .dropped_frames
                .map_or_else(|| "—".into(), |n| n.to_string()),
            s.player
                .sample
                .resident_bytes
                .map_or_else(|| "—".into(), |n| (n / 1024 / 1024).to_string()),
            s.player
                .sample
                .hw_decoder
                .as_deref()
                .unwrap_or("decoder pending")
        )))
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
                .opacity(0.45)
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
    rail("player-timeline", progress)
        .w_full()
        .label("Seek through video")
        .on_drag(|s: &mut State, drag, _| {
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
            rail(
                "player-volume",
                if s.player.muted {
                    0.0
                } else {
                    f64::from(s.player.volume) / 100.0
                },
            )
            .w(64.0)
            .label(format!("Volume {}% (↑ / ↓)", s.player.volume))
            .on_drag(|s: &mut State, drag, _| {
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
        .child(icon_button(
            s,
            "subtitles",
            "Cycle subtitles (C)",
            &CAPTIONS,
            |s| {
                s.player_action(PlayerAction::CycleSubtitles);
            },
        ))
        .child(icon_button(s, "audio", "Cycle audio track", &AUDIO, |s| {
            s.player_action(PlayerAction::CycleAudio);
        }))
        .child(fullscreen_button(s))
        .child(icon_button(s, "stop", "Stop playback", &STOP, |s| {
            s.player_action(PlayerAction::Stop);
        }));
    control_panel(s, buttons)
}

fn control_panel(s: &State, buttons: El) -> El {
    let controls = col()
        .key("player-controls")
        .w_full()
        .padding(12.0)
        .gap(6.0)
        .radius(8.0)
        .fill(panel())
        .child(timeline(s))
        .child(buttons);
    if s.player.fullscreen {
        controls
    } else {
        controls.child(label("Space / K  play / pause     ← / →  seek 5s     J / L  seek 10s     ↑ / ↓  volume     M  mute"))
    }
}

fn fullscreen_button(s: &State) -> El {
    icon_button(
        s,
        "fullscreen",
        if s.player.fullscreen {
            "Exit full screen (F / Esc)"
        } else {
            "Full screen (F)"
        },
        if s.player.fullscreen {
            &EXIT_FULLSCREEN
        } else {
            &FULLSCREEN
        },
        |s| s.player_action(PlayerAction::ToggleFullscreen),
    )
}

fn icon_button(
    state: &State,
    key: &'static str,
    title: &'static str,
    glyph: &'static Icon,
    f: impl Fn(&mut State) + 'static,
) -> El {
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
        })
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "A clamped UI fraction fits in f32"
)]
fn rail(key: &'static str, fraction: f64) -> El {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0) as f32
    } else {
        0.0
    };
    row()
        .key(key)
        .role("slider")
        .h(24.0)
        .center_y()
        .child(el().w(0.0).min_w(0.0).grow(fraction).h(4.0).fill(accent()))
        .child(el().w(10.0).h(10.0).radius(5.0).fill(theme().text))
        .child(
            el().w(0.0)
                .min_w(0.0)
                .grow(1.0 - fraction)
                .h(4.0)
                .fill(edge()),
        )
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "A clamped volume percentage fits in u32"
)]
fn volume_at(fraction: f32) -> u32 {
    (fraction.clamp(0.0, 1.0) * 100.0).round() as u32
}
