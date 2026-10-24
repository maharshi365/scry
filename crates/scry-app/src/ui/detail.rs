//! Metadata header, episode list, and season selection.
use super::episodes::season_label;
use super::{
    State,
    artwork::poster,
    components::{accent, action, excerpt, heading, label, panel, scroll},
    library_view, source_picker,
};
use rune::{
    El, ScrollSpec, col, row, text,
    tok::{self, TextStyle, type_},
};

pub(super) fn detail(s: &State) -> El {
    let mut header = col().gap(8.0).w_full();
    let mut episode_panel = None;
    if let Some(meta) = &s.detail {
        let info = std::iter::once(meta.year.as_str())
            .chain(meta.genres.iter().map(String::as_str))
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        header = header
            .child(text(info).font(type_::SECTION).color(accent()))
            .child(heading(meta.name.clone()))
            .child(label(excerpt(&meta.description, 240)))
            .child(library_view::detail_actions(s));
        if meta.kind == "series" {
            episode_panel = Some(episodes(s, meta));
        }
    }
    let sources = source_picker::view(s);
    let panels = if let Some(episodes) = episode_panel {
        row()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .gap(16.0)
            .child(episodes)
            .child(sources)
    } else {
        row().grow(1.0).min_h(0.0).min_w(0.0).child(sources)
    };
    let header = if let Some(meta) = s.detail.as_ref().filter(|meta| meta.poster.is_some()) {
        row()
            .gap(16.0)
            .child(poster(s, meta, 0).w(120.0).h(180.0))
            .child(header.grow(1.0).min_w(0.0))
    } else {
        header
    };
    col()
        .key("detail-page")
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .gap(16.0)
        .child(header)
        .child(panels)
}

fn episodes(s: &State, meta: &scry_addons::Meta) -> El {
    let t = tok::theme(true);
    let mut episodes = col().key("episodes").gap(5.0).w_full();
    let range = s.episode_range();
    for index in s.visible_episode_indices() {
        let video = &meta.videos[index];
        let selected = s.selected_video_id.as_deref() == Some(video.id.as_str());
        episodes = episodes.child(
            col()
                .key(format!("episode-{index}"))
                .role("button")
                .label(format!(
                    "Episode {} {}",
                    video.episode.unwrap_or(0),
                    video.title
                ))
                .padding(12.0)
                .gap(4.0)
                .radius(6.0)
                .hover(t.hover)
                .on_press(move |s: &mut State| s.select_episode(index))
                .w_full()
                .fill(if selected {
                    rune::oklch(0.31, 0.065, 220.0)
                } else {
                    t.card
                })
                .child(
                    text(format!(
                        "EPISODE {}{}",
                        video.episode.unwrap_or(0),
                        s.episode_label(&video.id)
                    ))
                    .font(type_::SECTION)
                    .color(if selected { accent() } else { t.text_secondary }),
                )
                .child(
                    text(if video.title.is_empty() {
                        format!("Episode {}", video.episode.unwrap_or(0))
                    } else {
                        video.title.clone()
                    })
                    .wrap()
                    .font(TextStyle::new(14.0, 500.0))
                    .color(t.text),
                ),
        );
    }
    let mut pages = row().gap(8.0);
    if range.start > 0 {
        pages = pages.child(action("episodes-prev", "Previous 100", |s| {
            s.move_episode_page(false);
        }));
    }
    pages = pages.child(label(format!(
        "Episodes {}–{} of {}",
        if range.is_empty() { 0 } else { range.start + 1 },
        range.end,
        s.season_episode_indices().len()
    )));
    if range.end < s.season_episode_indices().len() {
        pages = pages.child(action("episodes-next", "Next 100", |s| {
            s.move_episode_page(true);
        }));
    }
    col()
        .w(280.0)
        .min_w(240.0)
        .min_h(0.0)
        .gap(8.0)
        .child(
            text("EPISODES")
                .font(type_::SECTION)
                .color(t.text_secondary),
        )
        .child(season_selector(s))
        .child(scroll(
            format!(
                "episodes-scroll-{}-{}-{:?}",
                s.navigation.route_key,
                range.start,
                s.active_season()
            ),
            episodes,
        ))
        .child(pages)
}

fn season_selector(s: &State) -> El {
    let seasons = s.seasons();
    let current = s.active_season();
    let position = seasons
        .iter()
        .position(|season| *season == current)
        .unwrap_or(0);
    let mut bar = row().w_full().gap(6.0);
    if position > 0 {
        let previous = seasons[position - 1];
        bar = bar.child(action("season-prev", "‹", move |s| s.select_season(previous)).w(32.0));
    }
    bar = bar.child(
        action(
            "season-select",
            format!("{}  ▾", season_label(current)),
            |s| {
                s.season_menu_open = !s.season_menu_open;
            },
        )
        .w(0.0)
        .grow(1.0),
    );
    if let Some(next) = seasons.get(position + 1).copied() {
        bar = bar.child(action("season-next", "›", move |s| s.select_season(next)).w(32.0));
    }
    let mut selector = col().w_full().gap(6.0).child(bar);
    if s.season_menu_open {
        let mut choices = col().w_full().gap(4.0);
        for (index, season) in seasons.iter().copied().enumerate() {
            choices = choices.child(
                action(
                    format!("season-option-{index}"),
                    season_label(season),
                    move |s| {
                        s.select_season(season);
                    },
                )
                .w_full()
                .fill(if season == current {
                    rune::oklch(0.30, 0.05, 220.0)
                } else {
                    panel()
                }),
            );
        }
        let visible = u16::try_from(seasons.len().min(4)).unwrap_or(4);
        selector = selector.child(
            col()
                .key("season-menu")
                .w_full()
                .h(f32::from(visible) * 36.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(choices),
        );
    }
    selector
}
