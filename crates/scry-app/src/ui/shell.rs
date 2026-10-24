//! Sidebar, topbar, page dispatch, and shared status messages.
use super::{
    Page, State,
    addons::addons,
    controls::{action, icon_action, input, select_action},
    design::{
        icons,
        tokens::{self, type_},
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
        Page::Watching => library_view::watching(s),
        Page::Calendar => super::calendar::view(s, cx),
        Page::Library => library_view::view(s),
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
    let mut root = row()
        .key("scry")
        .w_full()
        .h_full()
        .fill(tokens::canvas())
        .on_secondary_press(|s: &mut State, p, _| {
            s.context_menu = Some((p, None));
        })
        .child(sidebar(s))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .h_full()
                .child(topbar(s))
                .child(content),
        );
    if let Some(menu) = super::context_menu::view(s, cx) {
        root = root.child(menu);
    }
    if s.addon_install_open {
        root = root.child(super::addons::install_dialog(s));
    }
    root
}

fn sidebar(s: &State) -> El {
    let mut sidebar = col()
        .key("sidebar")
        .w(180.0)
        .min_w(180.0)
        .h_full()
        .padding(14.0)
        .gap(8.0)
        .fill(tokens::sidebar())
        .child(
            row()
                .gap(8.0)
                .child(el().key("brand-logo").w(28.0).h(28.0).pixels(&s.logo))
                .child(text("SKRY").font(type_::WORDMARK).color(tokens::brand())),
        )
        .child(el().h(16.0));
    for (key, title, page) in [
        ("nav-watching", "Currently watching", Page::Watching),
        ("nav-library", "Library", Page::Library),
        ("nav-calendar", "Calendar", Page::Calendar),
        ("nav-addons", "Add-ons", Page::Addons),
    ] {
        sidebar = sidebar.child(
            select_action(key, title, s.navigation.page == page, move |s| {
                s.navigate(page);
            })
            .w_full(),
        );
        if page == Page::Watching {
            sidebar = sidebar
                .child(
                    select_action(
                        "movies",
                        "Movies",
                        s.navigation.page == Page::Discover && s.kind == "movie",
                        |s| s.choose_kind("movie"),
                    )
                    .w_full(),
                )
                .child(
                    select_action(
                        "series",
                        "TV shows",
                        s.navigation.page == Page::Discover && s.kind == "series",
                        |s| s.choose_kind("series"),
                    )
                    .w_full(),
                );
        }
    }
    sidebar.child(el().grow(1.0))
}

fn topbar(s: &State) -> El {
    row()
        .key("topbar")
        .h(68.0)
        .fill(tokens::topbar())
        .px(20.0)
        .gap(8.0)
        .center_y()
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
        .child(action("search-go", "Search", State::submit_search))
        .child(icon_action(
            "page-refresh",
            "Refresh current page",
            &icons::REFRESH,
            State::refresh_page,
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
}
