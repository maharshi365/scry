use super::super::design::tokens::{self, control, type_};
use super::{
    button::{Button, Variant},
    text::label,
};
use rune::{El, el, row, text};

/// Flat adjustment controls for use inside a panel. Callers own bounds and steps.
pub(in crate::ui) fn stepper<S: 'static>(
    key: &str,
    title: &str,
    value: impl Into<String>,
    enabled: bool,
    change: fn(&mut S, i32),
) -> El {
    row()
        .w_full()
        .center_y()
        .gap(12.0)
        .child(label(title).grow(1.0))
        .child(
            row()
                .key(format!("{key}-control"))
                .w(control::STEPPER_WIDTH)
                .h(control::HEIGHT)
                .center_y()
                .child(
                    Button::new(format!("{key}-less"), "−")
                        .variant(Variant::Ghost)
                        .enabled(enabled)
                        .on_press(move |s: &mut S| change(s, -1))
                        .w(control::HEIGHT),
                )
                .child(
                    el().key(format!("{key}-value"))
                        .grow(1.0)
                        .min_w(0.0)
                        .center()
                        .child(text(value).font(type_::LABEL).color(if enabled {
                            tokens::theme().text
                        } else {
                            tokens::disabled()
                        })),
                )
                .child(
                    Button::new(format!("{key}-more"), "+")
                        .variant(Variant::Ghost)
                        .enabled(enabled)
                        .on_press(move |s: &mut S| change(s, 1))
                        .w(control::HEIGHT),
                ),
        )
}
