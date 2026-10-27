use super::episodes::season_label;
use super::{
    State,
    controls::action,
    design::{
        icons,
        tokens::{accent, edge, panel, theme, type_, warning},
    },
    primitives::{
        button::{Button, Variant, menu_choice},
        surface::{card, dropdown, scroll},
        text::label,
    },
};
use rune::{Cx, El, col, el, icon, row, text};
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

// Filenames rarely contain spaces. Offer natural line breaks without changing
// the provider's stream payload or adding visible characters to the release.
fn wrap_filename(value: &str) -> String {
    let mut display = String::with_capacity(value.len());
    for character in value.chars() {
        display.push(character);
        if matches!(character, '.' | '_' | '-' | '/' | '\\') {
            display.push('\u{200b}');
        }
    }
    display
}

fn filter(
    key: impl Into<String>,
    title: String,
    selected: bool,
    f: impl Fn(&mut State) + 'static,
) -> El {
    menu_choice(key, title, selected, f)
}

fn providers(s: &State) -> Vec<(String, usize)> {
    let mut providers: Vec<(String, usize)> = Vec::new();
    for (addon, _) in &s.streams {
        if let Some((_, count)) = providers.iter_mut().find(|(name, _)| name == addon) {
            *count += 1;
        } else {
            providers.push((addon.clone(), 1));
        }
    }
    providers
}

fn visible_sources(s: &State) -> Vec<(usize, &String, &Stream)> {
    let sources = s
        .streams
        .iter()
        .enumerate()
        .filter_map(|(index, (addon, stream))| {
            s.stream_provider
                .as_ref()
                .is_none_or(|provider| provider == addon)
                .then_some((index, addon, stream))
        });
    if s.stream_best_first {
        super::source_ranking::rank_sources(
            sources,
            |(_, addon, stream)| (addon.as_str(), *stream),
            "",
        )
    } else {
        sources.collect()
    }
}

pub(super) fn provider_menu(s: &State, cx: &Cx<State>) -> Option<El> {
    if !s.source_menu_open {
        return None;
    }
    let anchor = cx.layout("source-select")?;
    let providers = providers(s);
    if providers.len() < 2 {
        return None;
    }
    let visible = u16::try_from(providers.len().saturating_add(1).min(4)).unwrap_or(4);
    let mut filters = col().key("source-filters").w_full().gap(6.0).child(filter(
        "source-all",
        "All add-ons".into(),
        s.stream_provider.is_none(),
        |s| {
            s.stream_provider = None;
            s.source_menu_open = false;
        },
    ));
    for (index, (provider, _)) in providers.into_iter().enumerate() {
        let selected = s.stream_provider.as_ref() == Some(&provider);
        filters = filters.child(filter(
            format!("source-provider-{index}"),
            provider.clone(),
            selected,
            move |s| {
                s.stream_provider = Some(provider.clone());
                s.source_menu_open = false;
            },
        ));
    }
    Some(
        dropdown(
            "source-menu",
            "source-select",
            anchor[2],
            f32::from(visible) * (super::design::tokens::control::HEIGHT + 6.0),
            |s: &mut State| s.source_menu_open = false,
        )
        .child(filters),
    )
}

fn sort_option(key: &str, title: &str, best: bool, selected: bool) -> El {
    menu_choice(key, title, selected, move |s: &mut State| {
        s.stream_best_first = best;
        s.source_sort_open = false;
    })
}

pub(super) fn sort_menu(s: &State, cx: &Cx<State>) -> Option<El> {
    if !s.source_sort_open {
        return None;
    }
    cx.layout("source-sort")?;
    Some(
        dropdown(
            "source-sort-menu",
            "source-sort",
            180.0,
            2.0 * (super::design::tokens::control::HEIGHT + 6.0),
            |s: &mut State| s.source_sort_open = false,
        )
        .child(
            col()
                .w_full()
                .gap(6.0)
                .child(sort_option(
                    "source-best",
                    "Best available",
                    true,
                    s.stream_best_first,
                ))
                .child(sort_option(
                    "source-original",
                    "Original order",
                    false,
                    !s.stream_best_first,
                )),
        ),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "Declarative source picker layout keeps provider filters and source-card bindings together"
)]
pub(super) fn view(s: &State) -> El {
    let t = theme();
    let sources = visible_sources(s);
    let visible = sources.len();
    let video = s.detail.as_ref().and_then(|meta| {
        meta.videos
            .iter()
            .find(|video| Some(&video.id) == s.selected_video_id.as_ref())
    });
    let mut controls = row().w_full().center_y().gap(8.0);
    if let Some(video) = video {
        let season = video.season;
        controls = controls.child(
            Button::new("season-select", season_label(season))
                .variant(Variant::Ghost)
                .on_press(|s: &mut State| {
                    s.source_menu_open = false;
                    s.source_sort_open = false;
                    s.season_menu_open = !s.season_menu_open;
                })
                .on_secondary_press(move |s: &mut State, point, _| {
                    s.open_season_menu(point, season);
                })
                .px(0.0)
                .hover(super::design::tokens::transparent())
                .cursor(rune::Cursor::Pointer)
                .gap(6.0)
                .child(icon(&icons::DOWN, 14.0).color(t.text_secondary)),
        );
    } else {
        controls = controls.child(text("Sources").font(type_::SECTION_TITLE).color(t.text));
    }
    controls = controls.child(el().grow(1.0)).child(
        Button::new(
            "source-sort",
            if s.stream_best_first {
                "Best available"
            } else {
                "Original order"
            },
        )
        .variant(Variant::Ghost)
        .on_press(|s: &mut State| {
            s.season_menu_open = false;
            s.source_menu_open = false;
            s.source_sort_open = !s.source_sort_open;
        })
        .px(0.0)
        .hover(super::design::tokens::transparent())
        .cursor(rune::Cursor::Pointer)
        .gap(6.0)
        .label("Sort sources")
        .child(icon(&icons::DOWN, 14.0).color(t.text_secondary)),
    );
    let mut header = col()
        .key("source-picker-header")
        .w_full()
        .gap(10.0)
        .child(controls);
    if let Some(video) = video {
        header = header.child(
            row()
                .w_full()
                .min_w(0.0)
                .center_y()
                .gap(8.0)
                .flex_wrap()
                .child(
                    text(if video.title.trim().is_empty() {
                        "Title unavailable"
                    } else {
                        video.title.trim()
                    })
                    .key("source-episode")
                    .wrap()
                    .font(type_::EMPTY_TITLE)
                    .color(t.text),
                )
                .child(
                    label(video.episode.map_or_else(
                        || "· Episode".into(),
                        |episode| format!("· Episode {episode}"),
                    ))
                    .key("source-episode-number")
                    .font(type_::BODY),
                ),
        );
    }
    let providers = providers(s);
    if providers.len() > 1 {
        header = header.child(
            row().key("source-controls").w_full().child(
                action(
                    "source-select",
                    s.stream_provider.as_deref().unwrap_or("All add-ons"),
                    |s| {
                        s.season_menu_open = false;
                        s.source_sort_open = false;
                        s.source_menu_open = !s.source_menu_open;
                    },
                )
                .w_full()
                .gap(6.0)
                .child(icon(&icons::DOWN, 14.0).color(t.text)),
            ),
        );
    }

    let mut list = col().key("detail-body").w_full().gap(8.0);
    for (index, message) in s.warnings.iter().enumerate() {
        list = list.child(
            col()
                .w_full()
                .padding(12.0)
                .radius(6.0)
                .fill(panel())
                .child(
                    text(message.clone())
                        .key(format!("stream-warning-{index}"))
                        .wrap()
                        .font(type_::BODY)
                        .color(warning()),
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
            ("Select an episode", "Choose an episode to see its streams.")
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
    for (index, addon, stream) in sources {
        let supported = stream.validate_playback().is_ok();
        let name = source_name(stream, addon);
        let mut details = col().w_full().min_w(0.0).gap(6.0).clip_rounded(0.0);
        let lines = release_lines(stream);
        if let Some(release) = lines.first() {
            details = details.child(
                text(wrap_filename(release))
                    .key(format!("stream-release-{index}"))
                    .w_full()
                    .wrap()
                    .font(type_::EMPHASIS)
                    .color(t.text),
            );
        }
        if lines.len() > 1 {
            details = details.child(
                label(lines[1..].join("\n"))
                    .w_full()
                    .key(format!("stream-details-{index}")),
            );
        }
        if !supported {
            details = details.child(label(
                "This source needs a playback method Scry does not support.",
            ));
        }
        let mut card = card()
            .key(format!("stream-{index}"))
            .w_full()
            .min_w(0.0)
            .padding(12.0)
            .gap(10.0)
            .radius(8.0)
            .border(1.0, super::design::tokens::source_edge())
            .fill(super::design::tokens::source_fill())
            .child(
                row()
                    .w_full()
                    .min_w(0.0)
                    .gap(10.0)
                    .child(
                        col().w(86.0).min_w(86.0).clip_rounded(0.0).gap(6.0).child(
                            text(name.clone())
                                .wrap()
                                .font(type_::ITEM_TITLE)
                                .color(t.text),
                        ),
                    )
                    .child(details.w(0.0).grow(1.0))
                    .child(
                        el().key(format!("stream-play-{index}"))
                            .w(28.0)
                            .min_w(28.0)
                            .h(28.0)
                            .radius(14.0)
                            .fill(if supported {
                                super::design::tokens::play_fill()
                            } else {
                                super::design::tokens::source_edge()
                            })
                            .center()
                            .child(if supported {
                                icon(&icons::PLAY, 14.0).color(accent())
                            } else {
                                text("—").font(type_::CAPTION).color(t.text_secondary)
                            }),
                    ),
            );
        if supported {
            let chosen = stream.clone();
            let title = s
                .detail
                .as_ref()
                .map(|meta| meta.name.clone())
                .unwrap_or_default();
            card = super::primitives::interaction::focus_ring(card.role("button").focusable(true))
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
    fn filename_breaks_preserve_text_and_separate_release_tokens() {
        let original = "Été.1976.REMASTERED.1080p.BluRay.DTS-HD_x264/Release.mkv";
        let wrapped = wrap_filename(original);
        assert!(wrapped.contains(".\u{200b}1976.\u{200b}REMASTERED"));
        assert!(wrapped.contains("-\u{200b}HD_\u{200b}x264/\u{200b}"));
        assert_eq!(wrapped.replace('\u{200b}', ""), original);
    }

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
