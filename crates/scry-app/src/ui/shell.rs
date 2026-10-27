//! Sidebar, topbar, and page dispatch.
use super::{
    Page, State,
    addons::addons,
    controls::{icon_action, input},
    design::{
        icons,
        tokens::{self, control, type_},
    },
    detail::detail,
    discover::discover,
    library_view,
    player::player,
    primitives::{
        button::{Button, Variant},
        input_group::input_group,
        surface::scroll,
    },
};
use rune::{Cx, El, col, el, row, text};

pub(super) fn shortcut(
    s: &mut State,
    event: &rune::KeyEvent,
    focus: super::shortcuts::Focus,
) -> bool {
    use super::shortcuts::{Binding, KeyMatch, Modifiers, resolve};
    let bindings = [
        Binding {
            key: KeyMatch::Function(11),
            modifiers: Modifiers::Plain,
            in_text: true,
            action: true,
        },
        Binding {
            key: KeyMatch::Escape,
            modifiers: Modifiers::Plain,
            in_text: false,
            action: false,
        },
    ];
    match resolve(&bindings, event, focus) {
        Some(true) => {
            s.toggle_app_fullscreen();
            true
        }
        Some(false) if s.app_fullscreen && !s.player.fullscreen => {
            s.toggle_app_fullscreen();
            true
        }
        _ => false,
    }
}

pub(super) fn view(s: &State, cx: &mut Cx<State>) -> El {
    if matches!(s.navigation.page, Page::Guide | Page::Player) && s.player.fullscreen {
        return fullscreen_player(s, cx.env().scale);
    }
    let mut content = col()
        .key("content")
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .gap(10.0)
        .padding(if s.navigation.page == Page::Detail {
            0.0
        } else {
            20.0
        });
    content = content.child(match s.navigation.page {
        Page::Discover => discover(s, cx),
        Page::Watching => library_view::watching(s, cx),
        Page::Calendar => super::calendar::view(s, cx),
        Page::Guide => super::guide::view(s, cx),
        Page::Library => library_view::view(s, cx),
        Page::Addons => addons(s),
        Page::Settings => super::settings::view(s),
        Page::Stats => super::stats::view(s),
        Page::Detail if s.detail_preview => row()
            .grow(1.0)
            .min_h(0.0)
            .min_w(0.0)
            .child(discover(s, cx).w(0.0).grow(1.0).padding(20.0))
            .child(super::detail::preview(s)),
        Page::Detail => detail(s),
        Page::Player => player(s, cx.env().scale),
    });
    let mut body = row().grow(1.0).min_w(0.0).min_h(0.0).w_full();
    if s.navigation.page != Page::Detail {
        body = body.child(sidebar(s));
    }
    let mut root = col()
        .key("scry")
        .w_full()
        .h_full()
        .fill(tokens::canvas())
        .on_secondary_press(|s: &mut State, p, _| {
            s.context_menu = Some((p, None));
        });
    if !s.app_fullscreen {
        root = root.child(topbar(s));
    }
    root = root
        .child(body.child(content))
        .children(super::stats::toast(s));
    if s.navigation.page == Page::Detail {
        if let Some(menu) = super::detail::season_menu(s, cx) {
            root = root.child(menu);
        }
        if let Some(menu) = super::source_picker::provider_menu(s, cx) {
            root = root.child(menu);
        }
        root = root.children(super::source_picker::sort_menu(s, cx));
    }
    if let Some(menu) = super::context_menu::view(s, cx) {
        root = root.child(menu);
    }
    if let Some(panel) = super::player::stats::panel(s) {
        root = root.child(panel);
    }
    if s.addon_install_open {
        root = root.child(super::addons::install_dialog(s));
    }
    if s.navigation.page == Page::Calendar && s.calendar.expanded_day.is_some() {
        root = root.child(super::calendar::day_dialog(s, cx));
    }
    root
}

fn fullscreen_player(s: &State, scale: f32) -> El {
    let mut content = col()
        .key("fullscreen-player")
        .w_full()
        .h_full()
        .fill(tokens::video())
        .child(player(s, scale));
    content = content.children(super::stats::toast(s));
    if let Some(panel) = super::player::stats::panel(s) {
        content = content.child(panel);
    }
    content
}

fn sidebar(s: &State) -> El {
    let mut sidebar = col()
        .key("sidebar")
        .w(240.0)
        .min_w(240.0)
        .h_full()
        .min_h(0.0)
        .padding(18.0)
        .gap(12.0)
        .fill(tokens::sidebar());
    let mut links = col().w_full().gap(12.0);
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
                        s.navigation.page == Page::Discover && s.kind == kind,
                        move |s| s.choose_kind(&kind),
                    )
                    .w_full(),
                );
            }
        }
    }
    sidebar = sidebar.child(scroll("sidebar-pages", links));
    let mut utilities = col().key("sidebar-utilities").w_full().gap(12.0);
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
    if s.app_fullscreen {
        sidebar = sidebar.child(
            icon_action(
                "app-exit-fullscreen",
                "Exit app fullscreen (F11)",
                &icons::EXIT_FULLSCREEN,
                State::toggle_app_fullscreen,
            )
            .w_full(),
        );
    }
    sidebar
}

fn nav_action(
    key: &str,
    title: &str,
    glyph: &'static rune::Icon,
    selected: bool,
    f: impl Fn(&mut State) + 'static,
) -> El {
    Button::new(key, title)
        .selected(selected)
        .frame()
        .h(52.0)
        .px(14.0)
        .gap(12.0)
        .child(rune::icon(glyph, 22.0).color(tokens::accent()))
        .child(
            text(title.to_owned())
                .font(type_::LABEL)
                .color(tokens::theme().text),
        )
        .on_press(f)
}

fn topbar(s: &State) -> El {
    let searching =
        s.navigation.page == Page::Discover && s.loading && !s.displayed_query.is_empty();
    let mut search = Button::new("search-go", "Search")
        .busy(searching)
        .frame()
        .h(control::INPUT_HEIGHT)
        .w(112.0)
        .gap(8.0);
    if searching {
        search = search.child(super::primitives::status::spinner("search-spinner", 16.0));
    }
    let search = search.child(text("Search").font(type_::BODY).color(tokens::theme().text));
    let search = if searching {
        search
    } else {
        search.on_press(State::submit_search)
    };
    row()
        .key("topbar")
        .w_full()
        .h(68.0)
        .fill(tokens::topbar())
        .px(14.0)
        .gap(8.0)
        .center_y()
        .child(
            el().key("brand-logo")
                .label("Scry")
                .w(28.0)
                .h(28.0)
                .pixels(&s.logo),
        )
        .child(el().w(8.0))
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
                    &s.search,
                    "Search movies and series…",
                    |s| &mut s.search,
                    State::submit_search,
                ))
                .child(search),
        )
        .child(
            icon_action(
                "page-refresh",
                "Refresh current page",
                &icons::REFRESH,
                State::refresh_page,
            )
            .w(control::INPUT_HEIGHT)
            .h(control::INPUT_HEIGHT),
        )
        .child(icon_action(
            "app-fullscreen",
            "App fullscreen (F11)",
            &icons::FULLSCREEN,
            State::toggle_app_fullscreen,
        ))
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
    .w(control::INPUT_HEIGHT)
    .h(control::INPUT_HEIGHT)
}
