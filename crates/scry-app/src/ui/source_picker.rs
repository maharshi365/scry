use super::episodes::season_label;
use super::{
    State,
    controls::{action, select_action},
    design::{
        icons,
        tokens::{accent, edge, panel, theme, type_, warning},
    },
    primitives::{
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
    select_action(key, title, selected, f).w_full()
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

#[expect(
    clippy::too_many_lines,
    reason = "Declarative source picker layout keeps provider filters and source-card bindings together"
)]
pub(super) fn view(s: &State) -> El {
    let t = theme();
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
                    .font(type_::SECTION_TITLE)
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
    let providers = providers(s);
    if providers.len() > 1 {
        header = header.child(
            action(
                "source-select",
                s.stream_provider.as_deref().unwrap_or("All add-ons"),
                |s| {
                    s.season_menu_open = false;
                    s.source_menu_open = !s.source_menu_open;
                },
            )
            .w_full()
            .gap(6.0)
            .child(icon(&icons::DOWN, 14.0).color(t.text)),
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
    for (index, (addon, stream)) in s.streams.iter().enumerate() {
        if s.stream_provider
            .as_ref()
            .is_some_and(|provider| provider != addon)
        {
            continue;
        }
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
            .border(1.0, edge().alpha(0.3))
            .fill(panel().alpha(0.4))
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
                                accent().alpha(0.12)
                            } else {
                                edge().alpha(0.3)
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
            card = card
                .role("button")
                .focusable(true)
                .focus_ring(2.0, 2.0, accent())
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
