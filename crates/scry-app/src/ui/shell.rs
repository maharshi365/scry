//! Sidebar, topbar, page dispatch, and shared status messages.
use super::{
    Page, State,
    addons::addons,
    components::{accent, action, excerpt, input, label},
    detail::detail,
    discover::discover,
    library_view,
    player::player,
};
use rune::{
    Cx, El, col, el, row, text,
    tok::{self, TextStyle, type_},
};

pub(super) fn view(s: &State, cx: &mut Cx<State>) -> El {
    let mut content = col()
        .key("content")
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .gap(10.0)
        .padding(20.0);
    content = content.child(match s.page {
        Page::Discover => discover(s, cx),
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
                .color(rune::oklch(0.78, 0.14, 25.0)),
        );
    }
    if let Some(error) = &s.library.error {
        content = content.child(label(format!("Library: {error}")));
    }
    if s.page == Page::Discover
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
    if s.page != Page::Detail && !s.status.is_empty() {
        content = content.child(label(s.status.clone()).key("status"));
    }
    row()
        .key("scry")
        .w_full()
        .h_full()
        .fill(rune::oklch(0.17, 0.018, 240.0))
        .child(sidebar(s))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .h_full()
                .child(topbar(s))
                .child(content),
        )
}

fn sidebar(s: &State) -> El {
    let mut sidebar = col()
        .key("sidebar")
        .w(128.0)
        .min_w(128.0)
        .h_full()
        .padding(14.0)
        .gap(8.0)
        .fill(rune::oklch(0.145, 0.018, 240.0))
        .child(
            row()
                .gap(8.0)
                .child(el().key("brand-logo").w(28.0).h(28.0).pixels(&s.logo))
                .child(
                    text("SKRY")
                        .font(TextStyle::new(19.0, 700.0).tracking(2.0))
                        .color(rune::color::hex("#D8ED96").unwrap_or_else(accent)),
                ),
        )
        .child(el().h(16.0));
    for (key, title, page) in [
        ("nav-discover", "Discover", Page::Discover),
        ("nav-library", "Library", Page::Library),
        ("nav-addons", "Add-ons", Page::Addons),
    ] {
        sidebar = sidebar.child(action(key, title, move |s| s.navigate(page)).w_full().fill(
            if s.page == page {
                rune::oklch(0.29, 0.05, 220.0)
            } else {
                rune::oklch(0.145, 0.018, 240.0)
            },
        ));
        if page == Page::Discover {
            sidebar = sidebar
                .child(
                    action("movies", "Movies", |s| s.choose_kind("movie"))
                        .w_full()
                        .fill(if s.page == Page::Discover && s.kind == "movie" {
                            rune::oklch(0.25, 0.035, 220.0)
                        } else {
                            rune::oklch(0.145, 0.018, 240.0)
                        }),
                )
                .child(
                    action("series", "TV shows", |s| s.choose_kind("series"))
                        .w_full()
                        .fill(if s.page == Page::Discover && s.kind == "series" {
                            rune::oklch(0.25, 0.035, 220.0)
                        } else {
                            rune::oklch(0.145, 0.018, 240.0)
                        }),
                );
        }
    }
    sidebar.child(el().grow(1.0))
}

fn topbar(s: &State) -> El {
    row()
        .key("topbar")
        .h(68.0)
        .fill(rune::oklch(0.15, 0.018, 240.0))
        .px(20.0)
        .gap(8.0)
        .center_y()
        .child(history_button("nav-back", "←", !s.back.is_empty(), false))
        .child(history_button(
            "nav-forward",
            "→",
            !s.forward.is_empty(),
            true,
        ))
        .child(input(
            "search",
            &s.search,
            "Search movies and series…",
            |s| &mut s.search,
        ))
        .child(action("search-go", "Search", State::submit_search))
}

fn history_button(key: &str, title: &str, enabled: bool, forward: bool) -> El {
    let control = row()
        .key(key)
        .h(32.0)
        .w(32.0)
        .center()
        .radius(7.0)
        .label(if forward {
            "Forward (Alt+Right)"
        } else {
            "Back (Alt+Left)"
        })
        .child(
            text(title)
                .font(TextStyle::new(20.0, 500.0))
                .color(if enabled {
                    tok::theme(true).text
                } else {
                    rune::oklch(0.4, 0.01, 275.0)
                }),
        );
    if enabled {
        control
            .role("button")
            .hover(tok::theme(true).hover)
            .on_press(move |s: &mut State| s.travel(forward))
    } else {
        control
    }
}
