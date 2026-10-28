use super::super::{
    Page, State,
    design::{
        icons::CHART,
        tokens::{self, type_},
    },
    primitives::{
        button::{Button, Variant},
        hover_overlay::{self, HoverOverlay},
        progress::bar,
    },
};
use rune::{El, col, row, text};

fn overlay(s: &mut State) -> &mut HoverOverlay {
    &mut s.player.stats_overlay
}

pub(super) fn trigger(s: &State) -> El {
    let trigger = Button::new("player-stats", "Stream stats")
        .variant(Variant::Ghost)
        .icon(&CHART)
        .selected(s.player.stats_overlay.is_open())
        .enabled(s.player.playing)
        .on_press_tx(|s: &mut State, tx| {
            s.player.stats_overlay.show();
            tx.instant();
        });
    hover_overlay::trigger(trigger, s.player.playing, overlay)
}

pub(in crate::ui) fn panel(s: &State) -> Option<El> {
    if !s.player.playing
        || !s.player.stats_overlay.is_open()
        || !(s.navigation.page == Page::Player
            || s.navigation.page == Page::Guide && s.player.fullscreen)
    {
        return None;
    }
    let sample = &s.player.sample;
    let mut content = col()
        .w_full()
        .gap(7.0)
        .child(
            text("Stream stats")
                .font(type_::CALENDAR_TITLE)
                .color(tokens::theme().text),
        )
        .child(pair(
            metric(
                "Video bitrate",
                number(sample.video_bitrate.map(|v| v / 1_000_000.0), "Mb/s"),
            ),
            metric("Buffered", number(sample.cache_s, "s")),
        ))
        .child(pair(
            metric(
                "Dropped frames",
                sample
                    .dropped_frames
                    .map_or_else(missing, |v| v.to_string()),
            ),
            metric(
                "Decoder",
                sample
                    .hw_decoder
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .unwrap_or("—")
                    .to_owned(),
            ),
        ));
    if s.player.torrent_source {
        content = content.child(torrent(s.player.torrent_stats.as_ref()));
    }
    Some(
        hover_overlay::panel(
            "player-stats-overlay",
            "player-stats",
            360.0,
            content,
            overlay,
        )
        .padding(14.0)
        .label("Stream stats")
        .on_layout(|s: &mut State, rect| {
            s.player.stats_overlay.rect = Some(rect);
            s.cover_player();
        }),
    )
}

fn pair(left: El, right: El) -> El {
    row()
        .w_full()
        .gap(20.0)
        .child(left.w(0.0).min_w(0.0).grow(1.0))
        .child(right.w(0.0).min_w(0.0).grow(1.0))
}

fn metric(title: &str, value: String) -> El {
    row()
        .w_full()
        .gap(6.0)
        .center_y()
        .child(
            text(title)
                .font(type_::CAPTION)
                .color(tokens::theme().text_secondary)
                .grow(1.0),
        )
        .child(
            text(value)
                .font(type_::CAPTION)
                .color(tokens::theme().text)
                .truncate(),
        )
}

fn missing() -> String {
    "—".into()
}

fn number(value: Option<f64>, unit: &str) -> String {
    value
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map_or_else(missing, |v| format!("{v:.1} {unit}"))
}

#[expect(
    clippy::cast_precision_loss,
    reason = "Display sizes and progress tolerate sub-byte precision loss"
)]
fn torrent(stats: Option<&scry_torrents::Stats>) -> El {
    let mut section = col()
        .key("player-torrent-stats")
        .w_full()
        .gap(9.0)
        .child(super::super::primitives::surface::divider())
        .child(
            row()
                .w_full()
                .gap(16.0)
                .child(
                    metric(
                        "↓ Download",
                        number(
                            stats
                                .and_then(|v| v.download_bytes_per_second)
                                .map(|v| v / 1_000_000.0),
                            "MB/s",
                        ),
                    )
                    .w(0.0)
                    .grow(1.0),
                )
                .child(
                    metric(
                        "↑ Upload",
                        number(
                            stats
                                .and_then(|v| v.upload_bytes_per_second)
                                .map(|v| v / 1_000_000.0),
                            "MB/s",
                        ),
                    )
                    .w(0.0)
                    .grow(1.0),
                )
                .child(
                    metric(
                        "Peers",
                        stats
                            .and_then(|v| v.peers)
                            .map_or_else(missing, |v| v.to_string()),
                    )
                    .w(0.0)
                    .grow(0.6),
                ),
        );
    let progress = stats
        .and_then(|v| Some((v.downloaded_bytes?.min(v.total_bytes), v.total_bytes)))
        .filter(|(_, total)| *total > 0);
    if let Some((downloaded, total)) = progress {
        let fraction = downloaded as f64 / total as f64;
        section = section
            .child(metric(
                &format!("{} / {}", bytes(downloaded), bytes(total)),
                format!("{:.0}%", fraction * 100.0),
            ))
            .child(
                bar("player-torrent-progress", fraction)
                    .grow(0.0)
                    .w_full()
                    .h(3.0)
                    .min_h(3.0),
            );
    } else {
        section = section.child(metric("Downloaded", missing()));
    }
    section
}

#[expect(
    clippy::cast_precision_loss,
    reason = "Human-readable byte sizes do not require exact integer precision"
)]
fn bytes(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1} GB", value as f64 / 1_000_000_000.0)
    } else if value >= 1_000_000 {
        format!("{:.1} MB", value as f64 / 1_000_000.0)
    } else if value >= 1000 {
        format!("{:.1} KB", value as f64 / 1000.0)
    } else {
        format!("{value} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_metrics_are_not_displayed_as_zero() {
        for value in [None, Some(f64::NAN), Some(f64::INFINITY), Some(-1.0)] {
            assert_eq!(number(value, "Mb/s"), "—");
        }
        assert_eq!(number(Some(0.0), "MB/s"), "0.0 MB/s");
        assert_eq!(number(Some(24.6), "Mb/s"), "24.6 Mb/s");
        assert_eq!(bytes(2_100_000_000), "2.1 GB");
    }
}
