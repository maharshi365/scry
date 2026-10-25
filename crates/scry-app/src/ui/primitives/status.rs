use super::super::design::{icons, tokens};
use rune::{El, Icon, el, icon};

pub(in crate::ui) fn spinning_icon(
    key: impl Into<String>,
    glyph: &'static Icon,
    size: f32,
    period: f64,
) -> El {
    icon(glyph, size)
        .key(key)
        .color(tokens::accent())
        .looping(rune::LoopSpec::rotation(period))
}

pub(in crate::ui) fn spinner(key: impl Into<String>, size: f32) -> El {
    spinning_icon(key, &icons::SPINNER, size, tokens::motion::SPINNER_PERIOD)
}

pub(in crate::ui) fn watched(key: impl Into<String>, size: f32) -> El {
    el().key(key)
        .role("img")
        .label("Watched")
        .w(size)
        .h(size)
        .center()
        .child(icon(&icons::CHECK, 16.0).color(tokens::success()))
}
