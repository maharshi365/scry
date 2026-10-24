use super::super::design::{
    icons,
    tokens::{accent, edge, panel, success, theme, type_},
};
use super::text::label;
use rune::{Color, El, Icon, ScrollSpec, col, el, icon, row, text};

pub(in crate::ui) fn card() -> El {
    surface(col())
}

pub(in crate::ui) fn surface(content: El) -> El {
    content.radius(12.0).fill(panel()).border(1.0, edge())
}

pub(in crate::ui) fn badge(key: impl Into<String>, value: impl Into<String>, color: Color) -> El {
    row()
        .key(key)
        .h(22.0)
        .px(8.0)
        .gap(4.0)
        .center()
        .radius(11.0)
        .fill(color.alpha(0.12))
        .border(1.0, color.alpha(0.25))
        .child(text(value).font(type_::BADGE).color(color))
}

pub(in crate::ui) fn watched_badge(key: impl Into<String>) -> El {
    badge(key, "Watched", success()).child(icon(&icons::CHECK, 12.0).color(success()))
}

pub(in crate::ui) fn empty_state(
    key: &str,
    title: &str,
    description: &str,
    glyph: &'static Icon,
    actions: El,
) -> El {
    col().key(key).w_full().grow(1.0).min_h(0.0).center().child(
        card()
            .w(440.0)
            .padding(32.0)
            .gap(18.0)
            .center()
            .radius(16.0)
            .child(
                el().w(64.0)
                    .h(64.0)
                    .radius(18.0)
                    .fill(accent().alpha(0.08))
                    .center()
                    .child(icon(glyph, 26.0).color(accent())),
            )
            .child(text(title).font(type_::EMPTY_TITLE).color(theme().text))
            .child(label(description))
            .child(actions),
    )
}

pub(in crate::ui) fn scroll(key: impl Into<String>, content: El) -> El {
    col()
        .key(key)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(content)
}
