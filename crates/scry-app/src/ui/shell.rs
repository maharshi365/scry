//! Sidebar, topbar, and page dispatch.
use super::{
    Page, State,
    addons::addons,
    controls::input,
    design::{
        icons,
        tokens::{self, control, type_},
    },
    detail::detail,
    discover::discover,
    library_view,
    player::player,
    primitives::{
        button::{Button, Variant, navigation_item},
        input_group::input_group,
        surface::{divider, scroll},
        text::eyebrow,
    },
};
use rune::{Cx, El, col, el, row, text};

pub(super) struct ShellState {
    pub host: scry_app::platform::Host,
    pub window: scry_app::platform::Window,
    pub host_error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    pub native_navigation: scry_app::platform::NavigationInput,
    pub context_menu: Option<(rune::PointerPress, Option<super::context_menu::Target>)>,
}

pub(super) fn view(s: &State, cx: &mut Cx<State>) -> El {
    if matches!(s.navigation.page, Page::Guide | Page::Player) && s.player.fullscreen {
        return fullscreen_player(s, cx.env().scale);
    }
    let content = page_content(s, cx);
    window_chrome(s, cx, content)
}

fn page_content(s: &State, cx: &mut Cx<State>) -> El {
    let mut content = col()
        .key("content")
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .gap(tokens::layout::PAGE_GAP)
        .padding(if s.navigation.page == Page::Detail {
            0.0
        } else {
            tokens::layout::PANEL_INSET
        });
    content = content.child(match s.navigation.page {
        Page::Discover => discover(s, cx),
        Page::Watching => library_view::watching(s, cx),
        Page::Calendar => super::calendar::view(s, cx),
        Page::Guide => super::guide::view(s, cx),
        Page::Library => library_view::view(s, cx),
        Page::MyMedia => super::my_media::view(s),
        Page::Addons => addons(s),
        Page::Settings => super::settings::view(s),
        Page::Stats => super::stats::view(s),
        Page::Detail if s.detail.preview => row()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .child(
                discover(s, cx)
                    .w(0.0)
                    .grow(1.0)
                    .padding(tokens::layout::PANEL_INSET),
            )
            .child(super::detail::preview(s)),
        Page::Detail => detail(s),
        Page::Player => player(s, cx.env().scale),
    });
    content
}

fn window_chrome(s: &State, cx: &mut Cx<State>, content: El) -> El {
    let body = row()
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .w_full()
        .child(sidebar(s));
    let mut root = col()
        .key("scry")
        .w_full()
        .h_full()
        .fill(tokens::canvas())
        .on_window_focus(on_window_focus)
        .on_secondary_press(|s: &mut State, p, _| {
            s.shell.context_menu = Some((p, None));
        });
    root = root.child(topbar(s, cx.titlebar_inset()));
    root = root
        .child(body.child(content))
        .children(super::stats::toast(s));
    root = root.children(super::discover::catalog_menu(s, cx));
    if s.navigation.page == Page::Detail {
        if let Some(menu) = super::detail::season_menu(s, cx) {
            root = root.child(menu);
        }
        if let Some(menu) = super::source_picker::provider_menu(s, cx) {
            root = root.child(menu);
        }
        root = root.children(super::source_picker::sort_menu(s, cx));
    }
    if let Some(menu) = super::context_menu::view(s) {
        root = root.child(menu);
    }
    if let Some(panel) = super::player::stats::panel(s) {
        root = root.child(panel);
    }
    if s.addons.install_open {
        root = root.child(super::addons::install_dialog(s));
    }
    if s.navigation.page == Page::Calendar && s.calendar.expanded_day.is_some() {
        root = root.child(super::calendar::day_dialog(s, cx));
    }
    root.children(super::player::subtitles::panel(s))
        .children(super::player::audio::panel(s))
        .children(super::settings::language_menu(s))
}

fn on_window_focus(s: &mut State, focused: bool, tx: &mut rune::Tx) {
    if !focused {
        s.discover.menu_open = false;
        s.player.subtitles.overlay.dismiss();
        s.player.audio.overlay.dismiss();
        s.player.stats_overlay.dismiss();
        tx.instant();
    }
}

fn fullscreen_player(s: &State, scale: f32) -> El {
    let mut content = col()
        .key("fullscreen-player")
        .w_full()
        .h_full()
        .fill(tokens::video())
        .on_window_focus(on_window_focus)
        .child(player(s, scale));
    content = content.children(super::stats::toast(s));
    if let Some(panel) = super::player::stats::panel(s) {
        content = content.child(panel);
    }
    content
        .children(super::player::subtitles::panel(s))
        .children(super::player::audio::panel(s))
}

fn sidebar(s: &State) -> El {
    let mut sidebar = col()
        .key("sidebar")
        .w(tokens::shell::SIDEBAR_WIDTH)
        .min_w(tokens::shell::SIDEBAR_WIDTH)
        .h_full()
        .min_h(0.0)
        .padding(tokens::shell::SIDEBAR_INSET)
        .gap(tokens::layout::PAGE_GAP)
        .fill(tokens::sidebar());
    let mut links = col()
        .w_full()
        .gap(tokens::shell::NAV_GAP)
        .child(eyebrow("COLLECTION").px(10.0).py(6.0));
    for (key, title, page, glyph) in [
        (
            "nav-watching",
            "Currently watching",
            Page::Watching,
            &icons::PLAY,
        ),
        ("nav-library", "Library", Page::Library, &icons::LIBRARY),
        ("nav-guide", "Guide", Page::Guide, &icons::TV),
        ("nav-calendar", "Calendar", Page::Calendar, &icons::CALENDAR),
    ] {
        links = links.child(
            nav_action(key, title, glyph, s.navigation.page == page, move |s| {
                s.navigate(page);
            })
            .w_full(),
        );
        if page == Page::Watching {
            for kind in s.catalog_categories() {
                let (key, glyph) = match kind.as_str() {
                    "movie" => ("movies".to_owned(), &icons::MOVIE),
                    "series" => ("series".to_owned(), &icons::TV),
                    _ => (format!("category-{kind}"), &icons::MOVIE),
                };
                links = links.child(
                    nav_action(
                        &key,
                        &super::catalog::category_label(&kind),
                        glyph,
                        s.navigation.page == Page::Discover && s.discover.kind == kind,
                        move |s| s.choose_kind(&kind),
                    )
                    .w_full(),
                );
            }
            links = links.child(
                nav_action(
                    "nav-my-media",
                    "My Media",
                    &icons::LIBRARY,
                    s.navigation.page == Page::MyMedia,
                    |s| s.navigate(Page::MyMedia),
                )
                .w_full(),
            );
        }
    }
    sidebar = sidebar.child(scroll("sidebar-pages", links));
    let mut utilities = col()
        .key("sidebar-utilities")
        .w_full()
        .gap(tokens::shell::NAV_GAP)
        .child(divider())
        .child(eyebrow("SYSTEM").px(10.0).py(6.0));
    for (key, title, page, glyph) in [
        ("nav-stats", "Stats for Nerds", Page::Stats, &icons::CHART),
        ("nav-addons", "Add-ons", Page::Addons, &icons::ADDONS),
        ("nav-settings", "Settings", Page::Settings, &icons::SETTINGS),
    ] {
        utilities = utilities.child(
            nav_action(key, title, glyph, s.navigation.page == page, move |s| {
                s.navigate(page);
            })
            .w_full(),
        );
    }
    sidebar = sidebar.child(utilities);
    sidebar.child(
        eyebrow(super::formatting::build_version(
            option_env!("SCRY_BUILD_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
        ))
        .px(10.0),
    )
}

fn nav_action(
    key: &str,
    title: &str,
    glyph: &'static rune::Icon,
    selected: bool,
    f: impl Fn(&mut State) + 'static,
) -> El {
    navigation_item(key, title, glyph, selected, f)
}

fn topbar(s: &State, native_inset: f32) -> El {
    let searching = s.navigation.page == Page::Discover
        && s.discover.request.loading
        && !s.discover.displayed_query.is_empty();
    let mut search_status = el()
        .w(control::COMPACT_ICON_SIZE)
        .h(control::COMPACT_ICON_SIZE);
    if searching {
        search_status = search_status
            .label("Searching")
            .child(super::primitives::status::spinner(
                "search-spinner",
                control::COMPACT_ICON_SIZE,
            ));
    }
    super::primitives::titlebar::titlebar(native_inset)
        .key("topbar")
        .child(
            text("SCRY")
                .key("brand-wordmark")
                .font(type_::BRAND)
                .color(tokens::theme().text),
        )
        .child(history_button(
            "nav-back",
            !s.navigation.back.is_empty(),
            false,
        ))
        .child(history_button(
            "nav-forward",
            !s.navigation.forward.is_empty(),
            true,
        ))
        .child(
            input_group()
                .grow(1.0)
                .child(input(
                    "search",
                    &s.discover.search,
                    "Search movies and series…",
                    |s| &mut s.discover.search,
                    State::submit_search,
                ))
                .child(search_status),
        )
}

fn history_button(key: &str, enabled: bool, forward: bool) -> El {
    Button::new(
        key,
        if forward {
            "Forward (Alt+Right)"
        } else {
            "Back (Alt+Left)"
        },
    )
    .variant(Variant::Ghost)
    .enabled(enabled)
    .icon(if forward { &icons::RIGHT } else { &icons::LEFT })
    .on_press(move |s: &mut State| s.travel(forward))
}
