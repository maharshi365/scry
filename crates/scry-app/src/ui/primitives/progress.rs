use super::super::design::tokens::{self, progress};
use rune::{El, el, row};

#[expect(
    clippy::cast_possible_truncation,
    reason = "A clamped UI fraction fits in f32"
)]
fn fraction(value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0) as f32
    } else {
        0.0
    }
}

pub(in crate::ui) fn bar(key: &str, value: f64) -> El {
    let fraction = fraction(value);
    row()
        .key(format!("{key}-track"))
        .role("progressbar")
        .label(format!("{:.0}% complete", fraction * 100.0))
        .grow(1.0)
        .h(progress::HEIGHT)
        .radius(progress::RADIUS)
        .fill(tokens::scrim())
        .child(
            el().key(format!("{key}-fill"))
                .grow(fraction)
                .h_full()
                .radius(progress::RADIUS)
                .fill(tokens::accent()),
        )
        .child(el().grow(1.0 - fraction).h_full())
}

/// Visual slider rail. Callers supply labels, sizes, and drag behavior.
pub(in crate::ui) fn slider(key: impl Into<String>, value: f64) -> El {
    let fraction = fraction(value);
    row()
        .key(key)
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
                .radius(progress::THUMB_SIZE / 2.0)
                .fill(tokens::theme().text),
        )
        .child(
            el().w(0.0)
                .min_w(0.0)
                .grow(1.0 - fraction)
                .h(progress::RAIL_HEIGHT)
                .fill(tokens::edge()),
        )
}
