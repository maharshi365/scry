//! Native video surface and playback controls.
use super::{
    Page, State,
    components::{accent, action, clock, label, metric, panel},
};
use rune::{
    El, Icon, Key, KeyEvent, Phase, col, el, icon, row, text,
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
    if s.playing {
        surface = surface.on_layout(move |s: &mut State, rect| {
            if s.playing && s.page == Page::Player {
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
                    text(s.playing_title.clone())
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
            metric(s.sample.cache_s),
            s.sample
                .dropped_frames
                .map_or_else(|| "—".into(), |n| n.to_string()),
            s.sample
                .resident_bytes
                .map_or_else(|| "—".into(), |n| (n / 1024 / 1024).to_string()),
            s.sample.hw_decoder.as_deref().unwrap_or("decoder pending")
        )))
}

fn timeline(s: &State) -> El {
    let progress = match (s.seek_preview.or(s.sample.position_s), s.sample.duration_s) {
        (Some(position), Some(duration)) if duration.is_finite() && duration > 0.0 => {
            (position / duration).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };
    rail("player-timeline", progress)
        .w_full()
        .label("Seek through video")
        .on_drag(|s: &mut State, drag, _| {
            if !s.playing || drag.w <= 0.0 {
                return;
            }
            let Some(duration) = s.sample.duration_s.filter(|n| n.is_finite() && *n > 0.0) else {
                return;
            };
            let target = f64::from((drag.x / drag.w).clamp(0.0, 1.0)) * duration;
            if drag.phase == Phase::Up {
                s.seek_to(target);
                s.seek_preview = None;
            } else {
                s.seek_preview = Some(target);
            }
        })
}

fn controls(s: &State) -> El {
    let position = s.seek_preview.or(s.sample.position_s);
    let buttons = row()
        .gap(4.0)
        .center_y()
        .child(icon_button(
            "pause",
            if s.sample.paused {
                "Play (Space / K)"
            } else {
                "Pause (Space / K)"
            },
            if s.sample.paused { &PLAY } else { &PAUSE },
            State::toggle_pause,
        ))
        .child(icon_button(
            "seek-back",
            "Back 10 seconds (J)",
            &BACK,
            |s| {
                s.command(&["seek", "-10", "relative"]);
            },
        ))
        .child(icon_button(
            "seek-forward",
            "Forward 10 seconds (L)",
            &FORWARD,
            |s| {
                s.command(&["seek", "10", "relative"]);
            },
        ))
        .child(icon_button(
            "mute",
            if s.muted { "Unmute (M)" } else { "Mute (M)" },
            if s.muted || s.volume == 0 {
                &MUTED
            } else {
                &VOLUME
            },
            State::toggle_mute,
        ))
        .child(
            rail(
                "player-volume",
                if s.muted {
                    0.0
                } else {
                    f64::from(s.volume) / 100.0
                },
            )
            .w(64.0)
            .label(format!("Volume {}% (↑ / ↓)", s.volume))
            .on_drag(|s: &mut State, drag, _| {
                if s.playing && drag.w > 0.0 {
                    s.set_volume(volume_at(drag.x / drag.w));
                }
            }),
        )
        .child(
            label(format!(
                "{} / {}",
                clock(position),
                clock(s.sample.duration_s)
            ))
            .key("player-time"),
        )
        .child(el().grow(1.0))
        .child(icon_button(
            "subtitles",
            "Cycle subtitles (C)",
            &CAPTIONS,
            |s| {
                s.command(&["cycle", "sid"]);
            },
        ))
        .child(icon_button("audio", "Cycle audio track", &AUDIO, |s| {
            s.command(&["cycle", "aid"]);
        }))
        .child(icon_button("stop", "Stop playback", &STOP, State::stop));
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
            if s.playing {
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

impl State {
    fn toggle_pause(&mut self) {
        if !self.playing {
            return;
        }
        self.sample.paused = !self.sample.paused;
        self.command(&[
            "set",
            "pause",
            if self.sample.paused { "yes" } else { "no" },
        ]);
    }

    fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.command(&["set", "mute", if self.muted { "yes" } else { "no" }]);
    }

    fn set_volume(&mut self, volume: u32) {
        self.volume = volume.min(100);
        self.muted = false;
        self.command(&["set", "mute", "no"]);
        self.command(&["set", "volume", &self.volume.to_string()]);
    }

    fn seek_to(&mut self, target: f64) {
        self.sample.position_s = Some(target);
        self.command(&["seek", &target.to_string(), "absolute"]);
    }
}

pub(super) fn on_key(s: &mut State, event: &KeyEvent) -> bool {
    let mods = event.mods;
    if s.page != Page::Player || !s.playing || mods.alt || mods.ctrl || mods.cmd || mods.logo {
        return false;
    }
    match &event.key {
        Key::Text(key) if key == " " || key.eq_ignore_ascii_case("k") => s.toggle_pause(),
        Key::Text(key) if key.eq_ignore_ascii_case("m") => s.toggle_mute(),
        Key::Text(key) if key.eq_ignore_ascii_case("j") => s.command(&["seek", "-10", "relative"]),
        Key::Text(key) if key.eq_ignore_ascii_case("l") => s.command(&["seek", "10", "relative"]),
        Key::Text(key) if key.eq_ignore_ascii_case("c") => s.command(&["cycle", "sid"]),
        Key::Left => s.command(&["seek", "-5", "relative"]),
        Key::Right => s.command(&["seek", "5", "relative"]),
        Key::Up => s.set_volume(s.volume.saturating_add(5)),
        Key::Down => s.set_volume(s.volume.saturating_sub(5)),
        _ => return false,
    }
    true
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
