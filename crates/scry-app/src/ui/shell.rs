//! Sidebar, topbar, page dispatch, and shared status messages.
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
    formatting::excerpt,
    library_view,
    player::player,
    primitives::{
        button::{Button, Variant},
        text::label,
    },
};
use rune::{Cx, El, col, el, row, text};

pub(super) fn view(s: &State, cx: &mut Cx<State>) -> El {
    if s.navigation.page == Page::Player && s.player.fullscreen {
        let mut content = col()
            .key("fullscreen-player")
            .w_full()
            .h_full()
            .fill(tokens::video())
            .child(player(s, cx.env().scale));
        if let Some(error) = &s.error {
            content = content.child(label(error.clone()));
        }
        return content;
    }
    let mut content = col()
        .key("content")
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .gap(10.0)
        .padding(20.0);
    content = content.child(match s.navigation.page {
        Page::Discover => discover(s, cx),
        Page::Watching => library_view::watching(s, cx),
        Page::Calendar => super::calendar::view(s, cx),
        Page::Library => library_view::view(s, cx),
        Page::Addons => addons(s),
        Page::Detail => detail(s),
        Page::Player => player(s, cx.env().scale),
    });
    if let Some(error) = &s.error {
        content = content.child(
            text(error.clone())
                .key("error")
                .font(type_::BODY)
                .color(tokens::danger()),
        );
    }
    if let Some(error) = &s.library.error {
        content = content.child(label(format!("Library: {error}")));
    }
    if s.navigation.page == Page::Discover
        && let Some(warning) = s.warnings.first()
    {
        content = content.child(label(format!(
            "Add-on notice: {}{}",
            excerpt(warning, 140),
            if s.warnings.len() > 1 {
                " · more providers reported a problem"
            } else {
                ""
            }
        )));
    }
    if !matches!(s.navigation.page, Page::Detail | Page::Player) && !s.status.is_empty() {
        content = content.child(label(s.status.clone()).key("status"));
    }
    let mut root = col()
        .key("scry")
        .w_full()
        .h_full()
        .fill(tokens::canvas())
        .on_secondary_press(|s: &mut State, p, _| {
            s.context_menu = Some((p, None));
        })
        .child(topbar(s))
        .child(
            row()
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .w_full()
                .child(sidebar(s))
                .child(content),
        );
    if s.navigation.page == Page::Detail {
        if let Some(menu) = super::detail::season_menu(s, cx) {
            root = root.child(menu);
        }
        if let Some(menu) = super::source_picker::provider_menu(s, cx) {
            root = root.child(menu);
        }
    }
    if let Some(menu) = super::context_menu::view(s, cx) {
        root = root.child(menu);
    }
    if s.addon_install_open {
        root = root.child(super::addons::install_dialog(s));
    }
    if s.navigation.page == Page::Calendar && s.calendar.expanded_day.is_some() {
        root = root.child(super::calendar::day_dialog(s, cx));
    }
    root
}

fn sidebar(s: &State) -> El {
    let mut sidebar = col()
        .key("sidebar")
        .w(240.0)
        .min_w(240.0)
        .h_full()
        .padding(18.0)
        .gap(12.0)
        .fill(tokens::sidebar());
    for (key, title, page, glyph) in [
        (
            "nav-watching",
            "Currently watching",
            Page::Watching,
            &icons::PLAY,
        ),
        ("nav-library", "Library", Page::Library, &icons::LIBRARY),
        ("nav-calendar", "Calendar", Page::Calendar, &icons::CALENDAR),
        ("nav-addons", "Add-ons", Page::Addons, &icons::ADDONS),
    ] {
        sidebar = sidebar.child(
            nav_action(key, title, glyph, s.navigation.page == page, move |s| {
                s.navigate(page);
            })
            .w_full(),
        );
        if page == Page::Watching {
            sidebar = sidebar
                .child(
                    nav_action(
                        "movies",
                        "Movies",
                        &icons::MOVIE,
                        s.navigation.page == Page::Discover && s.kind == "movie",
                        |s| s.choose_kind("movie"),
                    )
                    .w_full(),
                )
                .child(
                    nav_action(
                        "series",
                        "TV shows",
                        &icons::TV,
                        s.navigation.page == Page::Discover && s.kind == "series",
                        |s| s.choose_kind("series"),
                    )
                    .w_full(),
                );
        }
    }
    sidebar.child(el().grow(1.0))
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
        search = search.child(
            rune::icon(&icons::SPINNER, 16.0)
                .key("search-spinner")
                .color(tokens::accent())
                .looping(rune::LoopSpec::rotation(0.8)),
        );
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
        .child(input(
            "search",
            &s.search,
            "Search movies and series…",
            |s| &mut s.search,
            State::submit_search,
        ))
        .child(search)
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
