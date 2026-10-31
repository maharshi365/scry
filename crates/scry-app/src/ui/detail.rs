//! Metadata header, episode list, and season selection.
use super::episodes::season_label;
use super::{
    State,
    artwork::{poster, poster_overlay},
    controls::icon_action,
    design::{
        icons,
        tokens::{self, accent, theme, type_},
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

#[derive(Default)]
pub(super) struct DetailState {
    pub request: super::request::RequestState,
    pub meta: Option<scry_addons::Meta>,
    pub namespace: String,
    pub preview: bool,
    pub pending_episode: Option<String>,
    pub episodes: super::episodes::EpisodesState,
    pub sources: super::source_picker::SourcesState,
    pub generation: u64,
}

#[derive(Clone)]
pub(super) struct DetailSnapshot {
    meta: Option<scry_addons::Meta>,
    namespace: String,
    preview: bool,
    pending_episode: Option<String>,
    episodes: super::episodes::EpisodeSnapshot,
    request: super::request::RequestSnapshot,
    sources: super::request::RequestSnapshot,
}

impl DetailState {
    pub fn snapshot(&self) -> DetailSnapshot {
        DetailSnapshot {
            meta: self.meta.clone(),
            namespace: self.namespace.clone(),
            preview: self.preview,
            pending_episode: self.pending_episode.clone(),
            episodes: self.episodes.snapshot(),
            request: self.request.snapshot(),
            sources: self.sources.request.snapshot(),
        }
    }

    pub fn restore(&mut self, snapshot: DetailSnapshot) -> bool {
        self.meta = snapshot.meta;
        self.namespace = snapshot.namespace;
        self.preview = snapshot.preview;
        self.pending_episode = snapshot.pending_episode;
        self.episodes.restore(snapshot.episodes);
        self.sources.reset();
        self.sources.request.restore(snapshot.sources);
        self.request.restore(snapshot.request)
    }
}

const PREVIEW_WIDTH: f32 = tokens::detail::PREVIEW_WIDTH;
const PREVIEW_PADDING: f32 = tokens::detail::PREVIEW_INSET;

pub(super) fn detail(s: &State) -> El {
    let mut page = backdrop(s)
        .key("detail-page")
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0);
    let mut side = col()
        .key("detail-selection")
        .w(tokens::detail::SOURCE_WIDTH)
        .min_h(0.0)
        .padding(tokens::layout::PAGE_GAP)
        .gap(12.0)
        .fill(tokens::preview_panel());
    if let Some(meta) = &s.detail.meta {
        if meta.kind == "series" && s.detail.episodes.selected.is_none() {
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
        .meta
        .as_ref()
        .and_then(|meta| meta.background.as_ref())
        .and_then(|url| s.artwork.player.as_ref()?.get(url))
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
    let to_episodes = s.detail.episodes.selected.is_some()
        && s.detail
            .meta
            .as_ref()
            .is_some_and(|meta| meta.kind == "series");
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
    if s.shell.fullscreen {
        navigation = navigation.child(icon_action(
            "app-exit-fullscreen",
            "Exit app fullscreen (F11)",
            &icons::EXIT_FULLSCREEN,
            State::toggle_app_fullscreen,
        ));
    }
    let mut info = col()
        .w_full()
        .gap(tokens::layout::SECTION_GAP)
        .child(navigation);
    if let Some(meta) = &s.detail.meta {
        let title = if let Some(image) = meta
            .logo
            .as_ref()
            .and_then(|url| s.artwork.player.as_ref()?.get(url))
        {
            el().key("detail-logo")
                .w_full()
                .h(if compact { 72.0 } else { 96.0 })
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
            .join(" / ");
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
        .gap(tokens::layout::PAGE_GAP)
        .padding(if compact {
            PREVIEW_PADDING
        } else {
            tokens::layout::PANEL_INSET
        })
        .child(scroll("detail-info-scroll", info))
        .child(library_view::detail_actions(s))
}

fn chips(title: &str, values: &[String]) -> El {
    let mut section = col().w_full().gap(tokens::layout::COPY_GAP);
    if !values.is_empty() {
        section = section.child(label(title).font(type_::CAPTION));
        let mut line = row().w_full().gap(4.0).min_w(0.0).flex_wrap();
        for value in values.iter().take(6) {
            line = line.child(
                super::primitives::status::chip(value)
                    .max_w(PREVIEW_WIDTH - PREVIEW_PADDING * 2.0)
                    .min_w(0.0),
            );
        }
        section = section.child(line);
    }
    section
}

pub(super) fn quick_actions(s: &State, compact: bool) -> El {
    let trailer = s
        .detail
        .meta
        .as_ref()
        .and_then(|meta| meta.trailer_streams.first());
    let mut actions = input_group()
        .h(tokens::control::PILL_HEIGHT)
        .gap(tokens::layout::ACTION_GAP)
        .child(
            Button::new("title-trailer", "Watch trailer in browser")
                .size(super::primitives::button::Size::Pill)
                .enabled(trailer.is_some())
                .icon(&icons::TRAILER)
                .on_press_tx(State::open_trailer),
        );
    if compact {
        actions = actions.child(
            Button::new("title-play", "Show streams or episodes")
                .size(super::primitives::button::Size::Pill)
                .variant(Variant::Primary)
                .frame()
                .gap(10.0)
                .grow(1.0)
                .child(icon(&icons::PLAY, tokens::control::ICON_SIZE).color(tokens::primary_text()))
                .child(
                    text("Show")
                        .font(type_::LABEL)
                        .color(tokens::primary_text()),
                )
                .on_press(|s: &mut State| {
                    s.remember();
                    s.detail.preview = false;
                }),
        );
    }
    actions
}

impl State {
    pub(super) fn open_detail_from(&mut self, meta: scry_addons::Meta) {
        self.library.resume_request = None;
        if self.navigation.page == super::Page::Detail {
            self.detail.generation += 1;
            self.runtime.streams.invalidate();
        }
        self.navigate(super::Page::Detail);
        self.detail.meta = Some(meta.clone());
        self.detail.preview = false;
        self.detail.episodes = super::episodes::EpisodesState::default();
        self.detail.sources.reset();
        self.library.snapshot = None;
        self.detail.request = super::request::RequestState {
            loading: true,
            status: "Loading details…".into(),
            ..super::request::RequestState::default()
        };
        if let Some(services) = &mut self.runtime.services {
            services.detail(
                self.detail.generation,
                self.addons.installed.clone(),
                meta,
                self.detail.namespace.clone(),
            );
        }
    }

    pub(super) fn find_streams(&mut self, kind: String, id: String) {
        let generation = self
            .runtime
            .streams
            .start(super::request::StreamOwner::Detail);
        self.detail.sources.reset();
        self.detail.episodes.selected = Some(id.clone());
        self.refresh_library();
        self.detail.sources.request.loading = true;
        self.detail.sources.request.status = "Loading streams…".into();
        if let Some(services) = &mut self.runtime.services {
            services.streams(generation, self.addons.installed.clone(), kind, id);
        }
    }

    fn open_trailer(&mut self, tx: &mut rune::Tx) {
        let Some(id) = self
            .detail
            .meta
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
            self.detail.request.error = Some("The provider supplied an invalid trailer ID.".into());
            return;
        }
        let url = format!("https://www.youtube.com/watch?v={id}");
        if scry_app::platform::validate_browser_url(&url).is_err() || tx.open_url(&url).is_err() {
            self.detail.request.error = Some("Cannot open the trailer".into());
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
    let indices = s.season_episode_indices();
    let range = s.detail.episodes.range(indices.len());
    if range.is_empty() {
        episodes = episodes.child(label(if s.detail.episodes.search.text.trim().is_empty() {
            "No episodes in this season."
        } else {
            "No episodes match your search."
        }));
    }
    for (position, index) in indices.into_iter().take(range.end).enumerate() {
        let video = &meta.videos[index];
        let selected = s.detail.episodes.selected.as_deref() == Some(video.id.as_str());
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
            super::primitives::interaction::action(
                super::primitives::surface::Variant::Episode
                    .apply_selected(row(), selected)
                    .key(format!("episode-{index}")),
                selected,
            )
            .label(format!(
                "Episode {} {}",
                video.episode.unwrap_or(0),
                video.title
            ))
            .padding(12.0)
            .gap(8.0)
            .on_press(move |s: &mut State| s.select_episode(index))
            .on_secondary_press(move |s: &mut State, p, _| {
                let item = s.detail.meta.as_ref().and_then(|meta| {
                    meta.videos.get(index).map(|video| scry_library::Media {
                        key: super::library_view::key(
                            &super::library_view::key(
                                &s.detail.namespace,
                                scry_library::MediaKind::Series,
                                &meta.id,
                            )
                            .namespace,
                            scry_library::MediaKind::Episode,
                            &video.id,
                        ),
                        title: format!("{} · {}", meta.name, video.title),
                        parent: Some(super::library_view::key(
                            &s.detail.namespace,
                            scry_library::MediaKind::Series,
                            &meta.id,
                        )),
                    })
                });
                s.open_title_menu(p, item);
            })
            .w_full()
            .on_layout(move |s: &mut State, rect| {
                if s.detail.episodes.viewport.is_some_and(|viewport| {
                    rect[1] <= viewport[1] && rect[1] + rect[3] > viewport[1]
                }) {
                    let offset = position / super::EPISODES_PER_PAGE * super::EPISODES_PER_PAGE;
                    if s.detail.episodes.state_offset != Some(offset) {
                        s.detail.episodes.state_offset = Some(offset);
                        s.register_library_detail();
                    }
                }
                if position + 1 == range.end
                    && range.end == s.episode_range().end
                    && s.detail
                        .episodes
                        .viewport
                        .is_some_and(|viewport| rect[1] < viewport[1] + viewport[3] + 200.0)
                {
                    s.move_episode_page(true);
                }
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
            .on_layout(|s: &mut State, rect| s.detail.episodes.viewport = Some(rect)),
        )
}

fn episode_thumbnail(s: &State, video: &scry_addons::Video, index: usize) -> El {
    let surface = super::primitives::image::frame(
        &video.title,
        index,
        super::primitives::image::Variant::Episode,
    )
    .key(format!("episode-thumbnail-{index}"));
    if let Some(image) = video
        .thumbnail
        .as_ref()
        .and_then(|url| s.artwork.posters.as_ref()?.get(url))
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
        super::primitives::button::dropdown_trigger(
            "season-select",
            season_label(current),
            super::primitives::button::DropdownVariant::Transparent,
        )
        .on_press(|s: &mut State| {
            s.detail.sources.menu_open = false;
            s.detail.sources.sort_open = false;
            s.detail.episodes.menu_open = !s.detail.episodes.menu_open;
        })
        .on_secondary_press(move |s: &mut State, point, _| s.open_season_menu(point, current)),
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
    if !s.detail.episodes.menu_open {
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
    Some(dropdown(
        "season-menu",
        "season-select",
        anchor[2].max(200.0),
        choices,
        |s: &mut State| s.detail.episodes.menu_open = false,
    ))
}
