//! Metadata header, episode list, and season selection.
use super::episodes::season_label;
use super::{
    State,
    artwork::{poster, poster_overlay},
    controls::icon_action,
    design::{
        icons,
        tokens::{self, accent, panel, theme, type_},
    },
    library_view,
    primitives::{
        button::{Button, Variant, menu_choice},
        input_group::input_group,
        surface::{dropdown, scroll},
        text::label,
    },
    source_picker,
};
use rune::{Cx, El, col, el, icon, row, text};

const PREVIEW_WIDTH: f32 = 400.0;
const PREVIEW_PADDING: f32 = 24.0;

pub(super) fn detail(s: &State) -> El {
    let mut page = backdrop(s)
        .key("detail-page")
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0);
    let mut side = col()
        .key("detail-selection")
        .w(360.0)
        .min_h(0.0)
        .padding(20.0)
        .gap(12.0)
        .fill(tokens::preview_panel());
    if let Some(meta) = &s.detail {
        if meta.kind == "series" && s.selected_video_id.is_none() {
            side = side.child(episodes(s, meta));
        } else {
            side = side.child(source_picker::view(s).w_full());
        }
    }
    page = page.child(
        row()
            .w_full()
            .h_full()
            .min_h(0.0)
            .child(hero(s, false).grow(1.0).w(0.0).min_w(0.0))
            .child(side),
    );
    page
}

pub(super) fn preview(s: &State) -> El {
    backdrop(s)
        .key("title-preview")
        .w(PREVIEW_WIDTH)
        .h_full()
        .child(hero(s, true).w_full().h_full())
}

fn backdrop(s: &State) -> El {
    let mut surface = el().fill(tokens::canvas());
    if let Some(image) = s
        .detail
        .as_ref()
        .and_then(|meta| meta.background.as_ref())
        .and_then(|url| s.player_artwork.as_ref()?.get(url))
    {
        surface = surface.child(
            el().key("detail-background")
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .pixels_fit(&image, rune::SurfaceFit::Fill)
                .opacity(tokens::opacity::DETAIL_BACKDROP),
        );
    }
    surface
}

fn back_button(s: &State) -> El {
    let to_episodes = s.selected_video_id.is_some()
        && s.detail.as_ref().is_some_and(|meta| meta.kind == "series");
    icon_action(
        "detail-back",
        if to_episodes {
            "Back to episodes"
        } else {
            "Back to browsing"
        },
        &icons::LEFT,
        move |s| {
            if to_episodes {
                s.show_episodes();
            } else {
                s.travel(false);
            }
        },
    )
}

fn hero(s: &State, compact: bool) -> El {
    let mut navigation = input_group()
        .w_full()
        .child(back_button(s))
        .child(el().grow(1.0));
    if s.app_fullscreen {
        navigation = navigation.child(icon_action(
            "app-exit-fullscreen",
            "Exit app fullscreen (F11)",
            &icons::EXIT_FULLSCREEN,
            State::toggle_app_fullscreen,
        ));
    }
    let mut info = col().w_full().gap(24.0).child(navigation);
    if let Some(meta) = &s.detail {
        let title = if let Some(image) = meta
            .logo
            .as_ref()
            .and_then(|url| s.player_artwork.as_ref()?.get(url))
        {
            el().key("detail-logo")
                .w_full()
                .h(if compact { 100.0 } else { 140.0 })
                .pixels_fit(&image, rune::SurfaceFit::Fit)
        } else {
            text(meta.name.clone())
                .key("detail-title")
                .wrap()
                .font(type_::LOADING_TITLE)
                .color(theme().text)
        };
        let title =
            title.on_secondary_press(|s: &mut State, p, _| s.open_title_menu(p, s.library_media()));
        if meta.poster.is_some() && meta.logo.is_none() {
            info = info.child(
                row()
                    .gap(16.0)
                    .center_y()
                    .child(poster_overlay(
                        s,
                        poster(s, &meta.name, meta.poster.as_deref(), 0)
                            .w(80.0)
                            .h(120.0),
                        s.library_media().as_ref().map(State::poster_key),
                        "detail-poster",
                        120.0,
                    ))
                    .child(title.w(0.0).grow(1.0).min_w(0.0)),
            );
        } else {
            info = info.child(title);
        }
        let facts = [&meta.runtime, &meta.year, &meta.imdb_rating]
            .into_iter()
            .filter(|value| !value.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("     ·     ");
        info = info.child(text(facts).wrap().font(type_::SECTION).color(theme().text));
        for (title, values) in [
            ("GENRES", &meta.genres),
            ("CAST", &meta.cast),
            ("DIRECTORS", &meta.director),
        ] {
            if !values.is_empty() {
                info = info.child(chips(title, values));
            }
        }
        info = info.child(
            col()
                .gap(10.0)
                .w_full()
                .child(label("SUMMARY").font(type_::CAPTION))
                .child(
                    text(meta.description.clone())
                        .w_full()
                        .wrap()
                        .font(type_::BODY)
                        .color(theme().text),
                ),
        );
    }
    col()
        .min_h(0.0)
        .gap(20.0)
        .padding(if compact { PREVIEW_PADDING } else { 32.0 })
        .child(scroll("detail-info-scroll", info))
        .child(library_view::detail_actions(s))
}

fn chips(title: &str, values: &[String]) -> El {
    let mut section = col().w_full().gap(10.0);
    if !values.is_empty() {
        section = section.child(label(title).font(type_::CAPTION));
        let mut line = row().w_full().gap(8.0).min_w(0.0).flex_wrap();
        for value in values.iter().take(6) {
            line = line.child(
                el().padding(8.0)
                    .max_w(PREVIEW_WIDTH - PREVIEW_PADDING * 2.0)
                    .min_w(0.0)
                    .radius(18.0)
                    .fill(tokens::chip_fill())
                    .child(
                        text(value.clone())
                            .truncate()
                            .font(type_::EMPHASIS)
                            .color(theme().text),
                    ),
            );
        }
        section = section.child(line);
    }
    section
}

pub(super) fn quick_actions(s: &State, compact: bool) -> El {
    let trailer = s
        .detail
        .as_ref()
        .and_then(|meta| meta.trailer_streams.first());
    let mut actions = input_group().h(52.0).gap(10.0).child(
        Button::new("title-trailer", "Watch trailer in browser")
            .enabled(trailer.is_some())
            .icon(&icons::TRAILER)
            .on_press(State::open_trailer)
            .w(52.0)
            .h(52.0)
            .radius(26.0),
    );
    if compact {
        actions = actions.child(
            Button::new("title-play", "Show streams or episodes")
                .variant(Variant::Primary)
                .frame()
                .h(52.0)
                .radius(26.0)
                .gap(10.0)
                .grow(1.0)
                .child(icon(&icons::PLAY, 22.0).color(accent()))
                .child(text("Show").font(type_::LABEL).color(theme().text))
                .on_press(|s: &mut State| {
                    s.remember();
                    s.detail_preview = false;
                }),
        );
    }
    actions
}

impl State {
    fn open_trailer(&mut self) {
        let Some(id) = self
            .detail
            .as_ref()
            .and_then(|meta| meta.trailer_streams.first())
            .map(|trailer| &trailer.yt_id)
        else {
            return;
        };
        if id.is_empty()
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            self.error = Some("The provider supplied an invalid trailer ID.".into());
            return;
        }
        let url = format!("https://www.youtube.com/watch?v={id}");
        if let Err(error) = scry_app::platform::open_browser(&url) {
            self.error = Some(format!("Cannot open the trailer: {error}"));
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "Episode layout keeps primary and context actions beside their protocol identity"
)]
fn episodes(s: &State, meta: &scry_addons::Meta) -> El {
    let t = theme();
    let mut episodes = col().key("episodes").gap(8.0).w_full();
    let range = s.episode_range();
    if range.is_empty() {
        episodes = episodes.child(label(if s.episode_search.text.trim().is_empty() {
            "No episodes in this season."
        } else {
            "No episodes match your search."
        }));
    }
    for (position, index) in s
        .season_episode_indices()
        .into_iter()
        .take(range.end)
        .enumerate()
    {
        let video = &meta.videos[index];
        let selected = s.selected_video_id.as_deref() == Some(video.id.as_str());
        let mut status = row().w_full().h(22.0).center_y().gap(6.0).child(
            text(format!("EPISODE {}", video.episode.unwrap_or(0)))
                .grow(1.0)
                .font(type_::BADGE)
                .color(if selected { accent() } else { t.text_secondary }),
        );
        if s.episode_watched(&video.id) {
            status = status.child(super::primitives::status::watched(
                format!("episode-watched-{index}"),
                22.0,
            ));
        }
        episodes = episodes.child(
            super::primitives::interaction::focus_ring(
                row()
                    .key(format!("episode-{index}"))
                    .role("button")
                    .focusable(true)
                    .focus_selected(selected),
            )
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
                    tokens::primary_edge()
                } else {
                    tokens::subtle_edge()
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
            .on_layout(move |s: &mut State, rect| {
                if s.episode_viewport.is_some_and(|viewport| {
                    rect[1] <= viewport[1] && rect[1] + rect[3] > viewport[1]
                }) {
                    let offset = position / super::EPISODES_PER_PAGE * super::EPISODES_PER_PAGE;
                    if s.episode_state_offset != Some(offset) {
                        s.episode_state_offset = Some(offset);
                        s.register_library_detail();
                    }
                }
                if position + 1 == range.end
                    && range.end == s.episode_range().end
                    && s.episode_viewport
                        .is_some_and(|viewport| rect[1] < viewport[1] + viewport[3] + 200.0)
                {
                    s.move_episode_page(true);
                }
            })
            .fill(if selected {
                tokens::selected()
            } else {
                panel()
            })
            .child(episode_thumbnail(s, video, index))
            .child(
                col()
                    .w(0.0)
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(6.0)
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
                    )
                    .child(
                        label(
                            video
                                .released
                                .as_deref()
                                .unwrap_or("Release date unavailable")
                                .split('T')
                                .next()
                                .unwrap_or_default(),
                        )
                        .font(type_::CAPTION),
                    ),
            ),
        );
    }
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(8.0)
        .child(season_selector(s))
        .child(
            scroll(
                format!(
                    "episodes-scroll-{}-{}-{:?}",
                    s.navigation.route_key,
                    0,
                    s.active_season()
                ),
                episodes,
            )
            .on_layout(|s: &mut State, rect| s.episode_viewport = Some(rect)),
        )
}

fn episode_thumbnail(s: &State, video: &scry_addons::Video, index: usize) -> El {
    let surface = el()
        .key(format!("episode-thumbnail-{index}"))
        .w(96.0)
        .h(60.0)
        .radius(6.0)
        .fill(tokens::poster(index));
    if let Some(image) = video
        .thumbnail
        .as_ref()
        .and_then(|url| s.artwork.as_ref()?.get(url))
    {
        surface.pixels_fit(&image, rune::SurfaceFit::Fill)
    } else {
        surface
            .center()
            .child(icon(&icons::TV, 22.0).color(theme().text_secondary))
    }
}

fn season_selector(s: &State) -> El {
    let seasons = s.seasons();
    let current = s.active_season();
    let position = seasons
        .iter()
        .position(|season| *season == current)
        .unwrap_or(0);
    let mut bar = row().w_full().center_y().gap(8.0);
    if position > 0 {
        let previous = seasons[position - 1];
        bar = bar.child(
            icon_action("season-prev", "Previous season", &icons::LEFT, move |s| {
                s.select_season(previous);
            })
            .w(tokens::control::HEIGHT),
        );
    }
    bar = bar.child(
        Button::new("season-select", season_label(current))
            .variant(Variant::Ghost)
            .on_press(|s: &mut State| {
                s.source_menu_open = false;
                s.source_sort_open = false;
                s.season_menu_open = !s.season_menu_open;
            })
            .px(0.0)
            .hover(tokens::transparent())
            .cursor(rune::Cursor::Pointer)
            .on_secondary_press(move |s: &mut State, point, _| s.open_season_menu(point, current))
            .gap(6.0)
            .child(icon(&icons::DOWN, 14.0).color(theme().text_secondary)),
    );
    if let Some(next) = seasons.get(position + 1).copied() {
        bar = bar.child(
            icon_action("season-next", "Next season", &icons::RIGHT, move |s| {
                s.select_season(next);
            })
            .w(tokens::control::HEIGHT),
        );
    }
    bar
}

pub(super) fn season_menu(s: &State, cx: &Cx<State>) -> Option<El> {
    if !s.season_menu_open {
        return None;
    }
    let anchor = cx.layout("season-select")?;
    let seasons = s.seasons();
    let current = s.active_season();
    let mut choices = col().w_full().gap(4.0);
    for (index, season) in seasons.iter().copied().enumerate() {
        choices = choices.child(
            menu_choice(
                format!("season-option-{index}"),
                season_label(season),
                season == current,
                move |s: &mut State| {
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
    Some(
        dropdown(
            "season-menu",
            "season-select",
            anchor[2].max(200.0),
            f32::from(visible) * (tokens::control::HEIGHT + 4.0),
            |s: &mut State| s.season_menu_open = false,
        )
        .child(choices),
    )
}
