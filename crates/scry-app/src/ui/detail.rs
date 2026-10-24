//! Metadata header, episode list, and season selection.
use super::episodes::season_label;
use super::{
    State,
    artwork::{poster, poster_overlay},
    controls::{action, icon_action, select_action},
    design::{
        icons,
        tokens::{self, accent, edge, panel, theme, type_},
    },
    formatting::excerpt,
    library_view,
    primitives::{
        surface::{scroll, watched_badge},
        text::{heading, label},
    },
    source_picker,
};
use rune::{El, ScrollSpec, col, icon, row, text};

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
            .child(
                heading(meta.name.clone()).on_secondary_press(|s: &mut State, p, _| {
                    s.open_title_menu(p, s.library_media());
                }),
            )
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
            .child(poster_overlay(
                s,
                poster(s, &meta.name, meta.poster.as_deref(), 0)
                    .w(120.0)
                    .h(180.0),
                s.library_media().as_ref().map(State::poster_key),
                "detail-poster",
                180.0,
            ))
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

#[expect(
    clippy::too_many_lines,
    reason = "Episode layout keeps primary and context actions beside their protocol identity"
)]
fn episodes(s: &State, meta: &scry_addons::Meta) -> El {
    let t = theme();
    let mut episodes = col().key("episodes").gap(8.0).w_full();
    let range = s.episode_range();
    for index in s.visible_episode_indices() {
        let video = &meta.videos[index];
        let selected = s.selected_video_id.as_deref() == Some(video.id.as_str());
        let mut status = row().w_full().h(22.0).center_y().gap(6.0).child(
            text(format!("EPISODE {}", video.episode.unwrap_or(0)))
                .grow(1.0)
                .font(type_::BADGE)
                .color(if selected { accent() } else { t.text_secondary }),
        );
        if s.episode_watched(&video.id) {
            status = status.child(watched_badge(format!("episode-watched-{index}")));
        }
        episodes = episodes.child(
            col()
                .key(format!("episode-{index}"))
                .role("button")
                .focusable(true)
                .focus_selected(selected)
                .focus_ring(2.0, 2.0, accent())
                .label(format!(
                    "Episode {} {}",
                    video.episode.unwrap_or(0),
                    video.title
                ))
                .padding(12.0)
                .gap(8.0)
                .radius(9.0)
                .border(
                    1.0,
                    if selected {
                        accent().alpha(0.5)
                    } else {
                        edge().alpha(0.6)
                    },
                )
                .hover(t.hover)
                .on_press(move |s: &mut State| s.select_episode(index))
                .on_secondary_press(move |s: &mut State, p, _| {
                    let item = s.detail.as_ref().and_then(|meta| {
                        meta.videos.get(index).map(|video| scry_library::Media {
                            key: super::library_view::key(
                                &super::library_view::key(
                                    &s.detail_namespace,
                                    scry_library::MediaKind::Series,
                                    &meta.id,
                                )
                                .namespace,
                                scry_library::MediaKind::Episode,
                                &video.id,
                            ),
                            title: format!("{} · {}", meta.name, video.title),
                            parent: Some(super::library_view::key(
                                &s.detail_namespace,
                                scry_library::MediaKind::Series,
                                &meta.id,
                            )),
                        })
                    });
                    s.open_title_menu(p, item);
                })
                .w_full()
                .fill(if selected {
                    tokens::selected()
                } else {
                    panel()
                })
                .child(status)
                .child(
                    text(if video.title.trim().is_empty() {
                        "Title unavailable".to_owned()
                    } else {
                        video.title.trim().to_owned()
                    })
                    .key(format!("episode-title-{index}"))
                    .wrap()
                    .font(type_::ITEM)
                    .color(if video.title.trim().is_empty() {
                        t.text_secondary
                    } else {
                        t.text
                    }),
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
        bar = bar.child(
            icon_action("season-prev", "Previous season", &icons::LEFT, move |s| {
                s.select_season(previous);
            })
            .w(32.0),
        );
    }
    bar = bar.child(
        action("season-select", season_label(current), |s| {
            s.season_menu_open = !s.season_menu_open;
        })
        .w(0.0)
        .grow(1.0)
        .on_secondary_press(move |s: &mut State, point, _| s.open_season_menu(point, current))
        .gap(6.0)
        .child(icon(&icons::DOWN, 14.0).color(theme().text)),
    );
    if let Some(next) = seasons.get(position + 1).copied() {
        bar = bar.child(
            icon_action("season-next", "Next season", &icons::RIGHT, move |s| {
                s.select_season(next);
            })
            .w(32.0),
        );
    }
    let mut selector = col().w_full().gap(6.0).child(bar);
    if s.season_menu_open {
        let mut choices = col().w_full().gap(4.0);
        for (index, season) in seasons.iter().copied().enumerate() {
            choices = choices.child(
                select_action(
                    format!("season-option-{index}"),
                    season_label(season),
                    season == current,
                    move |s| {
                        s.select_season(season);
                    },
                )
                .w_full()
                .on_secondary_press(move |s: &mut State, point, _| {
                    s.open_season_menu(point, season);
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
