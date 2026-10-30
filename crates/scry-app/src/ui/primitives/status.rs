use super::super::design::{icons, tokens};
use rune::{El, Icon, el, icon, text};

pub(in crate::ui) fn chip(value: impl Into<String>) -> El {
    el().px(6.0)
        .py(3.0)
        .radius(tokens::radius::CHIP)
        .fill(tokens::chip_fill())
        .child(
            text(value)
                .truncate()
                .font(tokens::type_::EMPHASIS)
                .color(tokens::theme().text),
        )
}

pub(in crate::ui) fn provider_tile() -> El {
    el().w(tokens::badge::MEDALLION_SIZE)
        .h(tokens::badge::MEDALLION_SIZE)
        .radius(tokens::radius::PROVIDER)
        .fill(tokens::selected())
        .center()
}

pub(in crate::ui) fn watched_badge(key: impl Into<String>, size: f32) -> El {
    watched(key, size).radius(size / 2.0).fill(tokens::scrim())
}

pub(in crate::ui) fn play(key: impl Into<String>, enabled: bool) -> El {
    el().key(key)
        .w(tokens::badge::PLAY_SIZE)
        .min_w(tokens::badge::PLAY_SIZE)
        .h(tokens::badge::PLAY_SIZE)
        .radius(tokens::badge::PLAY_SIZE / 2.0)
        .fill(if enabled {
            tokens::play_fill()
        } else {
            tokens::source_edge()
        })
        .center()
        .child(if enabled {
            icon(&icons::PLAY, tokens::badge::PLAY_ICON).color(tokens::accent())
        } else {
            text("—")
                .font(tokens::type_::CAPTION)
                .color(tokens::theme().text_secondary)
        })
}

#[derive(Clone, Copy)]
pub(in crate::ui) enum MedallionVariant {
    Earned,
    Locked,
}

pub(in crate::ui) fn medallion(
    key: impl Into<String>,
    title: &str,
    glyph: &'static Icon,
    variant: MedallionVariant,
) -> El {
    let earned = matches!(variant, MedallionVariant::Earned);
    el().key(key)
        .role("img")
        .label(title)
        .w(tokens::badge::MEDALLION_SIZE)
        .h(tokens::badge::MEDALLION_SIZE)
        .radius(tokens::radius::SM)
        .fill(tokens::stats_badge_fill(earned))
        .border(
            tokens::border::WIDTH,
            if earned {
                tokens::stats_gold()
            } else {
                tokens::edge()
            },
        )
        .center()
        .child(icon(glyph, tokens::badge::MEDALLION_ICON).color(if earned {
            tokens::stats_gold()
        } else {
            tokens::theme().text_secondary
        }))
}

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
        .child(icon(&icons::CHECK, tokens::control::ICON_SIZE).color(tokens::success()))
}
