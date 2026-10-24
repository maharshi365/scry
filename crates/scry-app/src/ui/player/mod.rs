//! Native video surface and playback controls.
pub(crate) mod actions;
mod session;
pub(super) mod shortcuts;
pub(crate) mod state;
use super::{
    Page, State,
    components::{accent, action, clock, label, metric, panel},
};
use actions::PlayerAction;
use rune::{
    El, Icon, Phase, col, el, icon, row, text,
    tok::{self, TextStyle},
};
use scry_app::platform::VideoHost;

pub(super) fn player(s: &State, scale: f32) -> El {
    let t = tok::theme(true);
    let mut surface = el()
        .key("video")
        .w_full()
        .grow(1.0)
        .fill(rune::oklch(0.08, 0.0, 0.0));
    if s.player.playing {
        surface = surface.on_layout(move |s: &mut State, rect| {
            if s.player.playing && s.navigation.page == Page::Player {
                s.host.place(rect, scale);
            }
        });
    } else {
        surface = surface.child(label("Choose another source to continue."));
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
                        .font(TextStyle::new(20.0, 550.0))
                        .color(t.text)
                        .grow(1.0),
                )
                .child(action("player-back", "← Sources", |s| s.travel(false))),
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
            "seek-back",
            "Back 10 seconds (J)",
            &BACK,
            |s| {
                s.player_action(PlayerAction::SeekRelative(-10));
            },
        ))
        .child(icon_button(
            "seek-forward",
            "Forward 10 seconds (L)",
            &FORWARD,
            |s| {
                s.player_action(PlayerAction::SeekRelative(10));
            },
        ))
        .child(icon_button(
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
            "subtitles",
            "Cycle subtitles (C)",
            &CAPTIONS,
            |s| {
                s.player_action(PlayerAction::CycleSubtitles);
            },
        ))
        .child(icon_button("audio", "Cycle audio track", &AUDIO, |s| {
            s.player_action(PlayerAction::CycleAudio);
        }))
        .child(icon_button("stop", "Stop playback", &STOP, |s| {
            s.player_action(PlayerAction::Stop);
        }));
    col()
        .key("player-controls")
        .w_full()
        .padding(12.0)
        .gap(6.0)
        .radius(8.0)
        .fill(panel())
        .child(timeline(s))
        .child(buttons)
        .child(label("Space / K  play / pause     ← / →  seek 5s     J / L  seek 10s     ↑ / ↓  volume     M  mute"))
}

fn icon_button(
    key: &'static str,
    title: &'static str,
    glyph: &'static Icon,
    f: impl Fn(&mut State) + 'static,
) -> El {
    el().key(key)
        .role("button")
        .label(title)
        .w(36.0)
        .h(36.0)
        .center()
        .radius(6.0)
        .hover(tok::theme(true).hover)
        .on_press(move |s: &mut State| {
            if s.player.playing {
                f(s);
            }
        })
        .child(icon(glyph, 22.0).color(tok::theme(true).text))
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
        .child(el().w(10.0).h(10.0).radius(5.0).fill(tok::theme(true).text))
        .child(
            el().w(0.0)
                .min_w(0.0)
                .grow(1.0 - fraction)
                .h(4.0)
                .fill(super::components::edge()),
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

static PLAY: Icon = Icon {
    name: "player-play",
    view: 24.0,
    body: r#"<path d="M8 5v14l11-7Z" fill="currentColor"/>"#,
};
static PAUSE: Icon = Icon {
    name: "player-pause",
    view: 24.0,
    body: r#"<path d="M6 5h4v14H6zM14 5h4v14h-4z" fill="currentColor"/>"#,
};
static BACK: Icon = Icon {
    name: "player-back",
    view: 24.0,
    body: r#"<path d="m11 6-8 6 8 6V6Zm10 0-8 6 8 6V6Z" fill="currentColor"/>"#,
};
static FORWARD: Icon = Icon {
    name: "player-forward",
    view: 24.0,
    body: r#"<path d="m3 6 8 6-8 6V6Zm10 0 8 6-8 6V6Z" fill="currentColor"/>"#,
};
static VOLUME: Icon = Icon {
    name: "player-volume",
    view: 24.0,
    body: r#"<path d="M3 9h4l5-4v14l-5-4H3Z" fill="currentColor"/><path d="M16 8q4 4 0 8m3-11q7 7 0 14" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
static MUTED: Icon = Icon {
    name: "player-muted",
    view: 24.0,
    body: r#"<path d="M3 9h4l5-4v14l-5-4H3Z" fill="currentColor"/><path d="m16 9 6 6m0-6-6 6" stroke="currentColor" stroke-width="2"/>"#,
};
static CAPTIONS: Icon = Icon {
    name: "player-captions",
    view: 24.0,
    body: r#"<rect x="2" y="5" width="20" height="14" rx="2" fill="none" stroke="currentColor" stroke-width="2"/><path d="M10 9H6v6h4m8-6h-4v6h4" fill="none" stroke="currentColor" stroke-width="2"/>"#,
};
static AUDIO: Icon = Icon {
    name: "player-audio",
    view: 24.0,
    body: r#"<path d="M5 10v4m4-8v12m4-15v18m4-15v12m4-8v4" stroke="currentColor" stroke-width="2"/>"#,
};
static STOP: Icon = Icon {
    name: "player-stop",
    view: 24.0,
    body: r#"<rect x="6" y="6" width="12" height="12" rx="1" fill="currentColor"/>"#,
};
