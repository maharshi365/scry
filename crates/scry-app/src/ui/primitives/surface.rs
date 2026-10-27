use super::super::design::tokens::{self, accent, edge, panel, theme, type_};
use super::text::label;
use rune::{Align, El, Icon, ScrollSpec, Side, col, el, icon, text};

pub(in crate::ui) fn card() -> El {
    surface(col())
}

pub(in crate::ui) fn surface(content: El) -> El {
    content.radius(12.0).fill(panel()).border(1.0, edge())
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
                    .fill(tokens::empty_icon_fill())
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

/// Attach at the window root, after the trigger, to escape page clipping.
pub(in crate::ui) fn dropdown<S: 'static>(
    key: &str,
    trigger: &str,
    width: f32,
    height: f32,
    dismiss: impl Fn(&mut S) + 'static,
) -> El {
    card()
        .key(key)
        .role("menu")
        .w(width)
        .h(height + 12.0)
        .padding(6.0)
        .radius(8.0)
        .anchored(trigger, Side::Bottom, Align::Start, 6.0)
        .modal(true, dismiss)
        .focus_group()
        .focus_return(trigger)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
}
