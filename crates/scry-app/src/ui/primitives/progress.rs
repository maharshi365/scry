use super::super::design::tokens::{self, progress};
use rune::{Color, El, el, row};

#[expect(
    clippy::cast_possible_truncation,
    reason = "A clamped UI fraction fits in f32"
)]
pub(in crate::ui) fn fraction(value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0) as f32
    } else {
        0.0
    }
}

pub(in crate::ui) fn bar(key: &str, value: f64) -> El {
    indicator(key, value, Variant::Default, tokens::accent())
}

#[derive(Clone, Copy)]
pub(in crate::ui) enum Variant {
    Default,
    Compact,
}

pub(in crate::ui) fn indicator(key: &str, value: f64, variant: Variant, color: Color) -> El {
    let fraction = fraction(value);
    let fill_key = format!("{key}-fill");
    let (key, height, fill, label) = match variant {
        Variant::Default => (
            format!("{key}-track"),
            progress::HEIGHT,
            tokens::scrim(),
            format!("{:.0}% complete", fraction * 100.0),
        ),
        Variant::Compact => (
            key.to_owned(),
            progress::COMPACT_HEIGHT,
            tokens::stats_rail(),
            format!("{:.0}%", fraction * 100.0),
        ),
    };
    row()
        .key(key)
        .role("progressbar")
        .label(label)
        .w_full()
        .grow(1.0)
        .h(height)
        .radius(progress::RADIUS)
        .fill(fill)
        .child(
            el().key(fill_key)
                .w(0.0)
                .min_w(0.0)
                .grow(fraction)
                .h_full()
                .radius(progress::RADIUS)
                .fill(color),
        )
        .child(el().w(0.0).min_w(0.0).grow(1.0 - fraction).h_full())
}

/// Two complementary shares, with an empty rail when no measurements exist.
pub(in crate::ui) fn split(
    key: &str,
    value: f64,
    colors: [Color; 2],
    has_data: bool,
    label: impl Into<String>,
) -> El {
    let fraction = fraction(value);
    let bar = row()
        .key(key)
        .role("img")
        .label(label)
        .w_full()
        .h(progress::SPLIT_HEIGHT)
        .radius(progress::SPLIT_RADIUS)
        .fill(tokens::stats_rail());
    if !has_data {
        return bar;
    }
    bar.child(
        el().w(0.0)
            .min_w(0.0)
            .grow(fraction)
            .h_full()
            .fill(colors[0]),
    )
    .child(
        el().w(0.0)
            .min_w(0.0)
            .grow(1.0 - fraction)
            .h_full()
            .fill(colors[1]),
    )
}

/// Visual slider rail. Callers supply labels, sizes, and drag behavior.
pub(in crate::ui) fn slider(key: impl Into<String>, value: f64) -> El {
    buffered_slider(key, value, None)
}

pub(in crate::ui) fn buffered_slider(
    key: impl Into<String>,
    value: f64,
    buffered: Option<(f64, f64)>,
) -> El {
    let key = key.into();
    let fraction = fraction(value);
    let buffered_end = buffered.map_or(fraction, |(_, end)| self::fraction(end).max(fraction));
    row()
        .key(key.clone())
        .role("slider")
        .h(progress::SLIDER_HEIGHT)
        .center_y()
        .child(
            el().w(0.0)
                .min_w(0.0)
                .grow(fraction)
                .h(progress::RAIL_HEIGHT)
                .fill(tokens::accent()),
        )
        .child(
            el().w(progress::THUMB_SIZE)
                .h(progress::THUMB_SIZE)
                .radius(tokens::radius::SM)
                .fill(tokens::theme().text),
        )
        .child(
            el().key(format!("{key}-buffered"))
                .w(0.0)
                .min_w(0.0)
                .grow(buffered_end - fraction)
                .h(progress::RAIL_HEIGHT)
                .fill(tokens::theme().text)
                .opacity(tokens::opacity::BUFFERED),
        )
        .child(
            el().w(0.0)
                .min_w(0.0)
                .grow(1.0 - buffered_end)
                .h(progress::RAIL_HEIGHT)
                .fill(tokens::edge()),
        )
}
