//! One control row for related inputs, selectors, and action buttons.
use super::super::design::tokens::control;
use rune::{El, row};

pub(in crate::ui) fn input_group() -> El {
    row()
        .h(control::HEIGHT)
        .min_h(control::HEIGHT)
        .gap(6.0)
        .center_y()
        .min_w(0.0)
}
