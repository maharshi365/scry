//! Native video surface and playback controls.
use super::{
    Page, State,
    components::{action, clock, label, metric},
};
use rune::{
    El, col, el, row, text,
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
            "{} / {}   ·   cache {}s   ·   drops {}   ·   RAM {} MiB   ·   {}",
            clock(s.sample.position_s),
            clock(s.sample.duration_s),
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

fn controls(s: &State) -> El {
    row()
        .gap(8.0)
        .center_y()
        .child(action(
            "pause",
            if s.sample.paused { "Resume" } else { "Pause" },
            |s| s.command(&["cycle", "pause"]),
        ))
        .child(action("seek-back", "−10s", |s| {
            s.command(&["seek", "-10", "relative"]);
        }))
        .child(action("seek-forward", "+10s", |s| {
            s.command(&["seek", "10", "relative"]);
        }))
        .child(action("volume-down", "Vol −", |s| {
            s.volume = s.volume.saturating_sub(10);
            let v = s.volume.to_string();
            s.command(&["set", "volume", &v]);
        }))
        .child(label(format!("{}%", s.volume)))
        .child(action("volume-up", "Vol +", |s| {
            s.volume = (s.volume + 10).min(100);
            let v = s.volume.to_string();
            s.command(&["set", "volume", &v]);
        }))
        .child(action("subtitles", "Subtitles ↻", |s| {
            s.command(&["cycle", "sid"]);
        }))
        .child(action("audio", "Audio ↻", |s| {
            s.command(&["cycle", "aid"]);
        }))
        .child(action("stop", "Stop", State::stop))
}
