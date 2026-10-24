use super::episodes::season_label;
use super::{
    State,
    components::{accent, action, badge, edge, label, panel, scroll},
};
use rune::{
    El, ScrollSpec, col, el, row, text,
    tok::{self, TextStyle, type_},
};
use scry_addons::Stream;

// Add-on names often put the quality on a second line. Keep that information
// together, without interpreting provider-specific text as verified metadata.
fn source_name(stream: &Stream, addon: &str) -> String {
    let name = stream.name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        addon.to_owned()
    } else {
        name
    }
}

fn release_lines(stream: &Stream) -> Vec<&str> {
    let title = if stream.title.trim().is_empty() {
        &stream.description
    } else {
        &stream.title
    };
    title
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn filter(
    key: impl Into<String>,
    title: String,
    selected: bool,
    f: impl Fn(&mut State) + 'static,
) -> El {
    action(key, title, f)
        .w_full()
        .fill(if selected {
            rune::oklch(0.30, 0.05, 220.0)
        } else {
            panel()
        })
        .border(1.0, if selected { accent() } else { edge() })
}

#[expect(
    clippy::too_many_lines,
    reason = "Declarative source picker layout keeps provider filters and source-card bindings together"
)]
pub(super) fn view(s: &State) -> El {
    let t = tok::theme(true);
    let visible = s
        .streams
        .iter()
        .filter(|(addon, _)| {
            s.stream_provider
                .as_ref()
                .is_none_or(|provider| provider == addon)
        })
        .count();
    let count = if s.stream_provider.is_some() {
        format!("{visible} of {} sources", s.streams.len())
    } else {
        format!("{visible} source{}", if visible == 1 { "" } else { "s" })
    };
    let mut header = col().key("source-picker-header").w_full().gap(10.0).child(
        row()
            .w_full()
            .center_y()
            .gap(12.0)
            .child(
                text("Sources")
                    .font(TextStyle::new(18.0, 600.0))
                    .color(t.text)
                    .grow(1.0),
            )
            .child(
                label(if s.loading {
                    format!("{count} · searching…")
                } else {
                    count
                })
                .key("source-count"),
            ),
    );
    if let Some(video) = s.detail.as_ref().and_then(|meta| {
        meta.videos
            .iter()
            .find(|video| Some(&video.id) == s.selected_video_id.as_ref())
    }) {
        header = header.child(
            label(format!(
                "{} · Episode {}{}",
                season_label(video.season),
                video.episode.unwrap_or(0),
                if video.title.trim().is_empty() {
                    String::new()
                } else {
                    format!(" · {}", video.title.trim())
                }
            ))
            .key("source-episode"),
        );
    }
    let mut providers: Vec<(String, usize)> = Vec::new();
    for (addon, _) in &s.streams {
        if let Some((_, count)) = providers.iter_mut().find(|(name, _)| name == addon) {
            *count += 1;
        } else {
            providers.push((addon.clone(), 1));
        }
    }
    if providers.len() > 1 {
        header = header.child(
            action(
                "source-select",
                format!(
                    "{}  ▾",
                    s.stream_provider.as_deref().unwrap_or("All add-ons")
                ),
                |s| {
                    s.source_menu_open = !s.source_menu_open;
                },
            )
            .w_full(),
        );
    }
    if providers.len() > 1 && s.source_menu_open {
        let visible = u16::try_from(providers.len().saturating_add(1).min(4)).unwrap_or(4);
        let menu_height = f32::from(visible) * 38.0;
        let mut filters = col().key("source-filters").w_full().gap(6.0).child(filter(
            "source-all",
            format!("All add-ons · {}", s.streams.len()),
            s.stream_provider.is_none(),
            |s| {
                s.stream_provider = None;
                s.source_menu_open = false;
            },
        ));
        for (index, (provider, count)) in providers.into_iter().enumerate() {
            let selected = s.stream_provider.as_ref() == Some(&provider);
            filters = filters.child(filter(
                format!("source-provider-{index}"),
                format!("{provider} · {count}"),
                selected,
                move |s| {
                    s.stream_provider = Some(provider.clone());
                    s.source_menu_open = false;
                },
            ));
        }
        header = header.child(
            col()
                .w_full()
                .h(menu_height)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(filters),
        );
    }

    let mut list = col().key("detail-body").w_full().gap(8.0);
    for (index, warning) in s.warnings.iter().enumerate() {
        list = list.child(
            col()
                .w_full()
                .padding(12.0)
                .radius(6.0)
                .fill(panel())
                .child(
                    text(warning.clone())
                        .key(format!("stream-warning-{index}"))
                        .wrap()
                        .font(type_::BODY)
                        .color(rune::oklch(0.8, 0.12, 70.0)),
                ),
        );
    }
    if visible == 0 {
        let (title, message) = if s.loading {
            (
                "Finding streams…",
                "Sources will appear as add-ons respond.",
            )
        } else if s.detail.as_ref().is_some_and(|meta| meta.kind == "series")
            && s.selected_video_id.is_none()
        {
            (
                "Select an episode",
                "Choose an episode on the left to see its streams.",
            )
        } else if s.stream_provider.is_some() && !s.streams.is_empty() {
            (
                "No sources from this add-on",
                "Choose All add-ons to see the other sources.",
            )
        } else {
            (
                "No streams found",
                "Try another episode or install a compatible stream add-on.",
            )
        };
        list = list.child(
            col()
                .key("source-empty")
                .w_full()
                .padding(24.0)
                .gap(8.0)
                .radius(8.0)
                .border(1.0, edge())
                .fill(panel())
                .child(text(title).font(type_::TITLE).color(t.text))
                .child(label(message)),
        );
    }
    for (index, (addon, stream)) in s.streams.iter().enumerate() {
        if s.stream_provider
            .as_ref()
            .is_some_and(|provider| provider != addon)
        {
            continue;
        }
        let supported = stream.validate_playback().is_ok();
        let name = source_name(stream, addon);
        let transport = if stream.url.is_some() {
            "Direct stream"
        } else if stream.info_hash.is_some() {
            "Torrent · connects to peers"
        } else {
            "External source"
        };
        let mut details = col().w_full().min_w(0.0).gap(6.0);
        let lines = release_lines(stream);
        if let Some(release) = lines.first() {
            details = details.child(
                text(*release)
                    .key(format!("stream-release-{index}"))
                    .w_full()
                    .wrap()
                    .font(TextStyle::new(13.0, 500.0))
                    .color(t.text),
            );
        }
        if lines.len() > 1 {
            details =
                details.child(label(lines[1..].join("\n")).key(format!("stream-details-{index}")));
        }
        if name != *addon {
            details = details
                .child(label(format!("Provided by {addon}")).font(TextStyle::new(11.0, 400.0)));
        }
        if !supported {
            details = details.child(label(
                "This source needs a playback method Scry does not support.",
            ));
        }
        let transport_color = if stream.info_hash.is_some() {
            rune::oklch(0.78, 0.09, 80.0)
        } else {
            accent()
        };
        let mut card = col()
            .key(format!("stream-{index}"))
            .w_full()
            .min_w(0.0)
            .padding(16.0)
            .gap(10.0)
            .radius(10.0)
            .border(1.0, edge().alpha(0.7))
            .fill(panel())
            .child(
                row()
                    .w_full()
                    .center_y()
                    .gap(10.0)
                    .child(
                        text(name.clone())
                            .w(0.0)
                            .grow(1.0)
                            .min_w(0.0)
                            .truncate()
                            .font(TextStyle::new(14.0, 600.0))
                            .color(t.text),
                    )
                    .child(badge(
                        format!("stream-transport-{index}"),
                        transport,
                        transport_color,
                    ))
                    .child(
                        el().w(28.0)
                            .h(28.0)
                            .radius(14.0)
                            .fill(if supported {
                                accent().alpha(0.12)
                            } else {
                                edge().alpha(0.3)
                            })
                            .center()
                            .child(
                                text(if supported { "▶" } else { "—" })
                                    .font(TextStyle::new(11.0, 500.0))
                                    .color(if supported {
                                        accent()
                                    } else {
                                        t.text_secondary
                                    }),
                            ),
                    ),
            )
            .child(details);
        if supported {
            let chosen = stream.clone();
            let title = s
                .detail
                .as_ref()
                .map(|meta| meta.name.clone())
                .unwrap_or_default();
            card = card
                .role("button")
                .label(format!("Play {name} from {addon}"))
                .hover(t.hover)
                .on_press(move |s: &mut State| s.play(chosen.clone(), title.clone()));
        }
        list = list.child(card);
    }
    col()
        .key("source-picker")
        .w(0.0)
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .gap(12.0)
        .child(header)
        .child(scroll(
            format!("sources-scroll-{}", s.navigation.route_key),
            list,
        ))
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Invalid source fixtures must fail the test immediately"
)]
mod tests {
    use super::*;

    #[test]
    fn presentation_preserves_quality_and_release_details_without_guessing() {
        let stream: Stream = serde_json::from_value(serde_json::json!({
            "name": "Provider\n4K HDR", "title": "  Film.2026.2160p.mkv\r\n\n  12 GB · 30 peers\nEnglish ",
            "description": "Not the release title"
        })).unwrap();
        assert_eq!(source_name(&stream, "Add-on"), "Provider 4K HDR");
        assert_eq!(
            release_lines(&stream),
            ["Film.2026.2160p.mkv", "12 GB · 30 peers", "English"]
        );
    }

    #[test]
    fn blank_provider_fields_use_addon_and_description() {
        let stream: Stream = serde_json::from_value(serde_json::json!({
            "name": " \n", "title": " \n", "description": "Release\nAudio details"
        }))
        .unwrap();
        assert_eq!(source_name(&stream, "Add-on"), "Add-on");
        assert_eq!(release_lines(&stream), ["Release", "Audio details"]);
    }
}
