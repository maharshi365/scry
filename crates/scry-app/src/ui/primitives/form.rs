use super::{
    super::design::tokens::{self, type_},
    text::label,
};
use rune::{El, col, row, text};

pub(in crate::ui) fn choices() -> El {
    row().min_w(0.0).gap(8.0).center_y().flex_wrap()
}

pub(in crate::ui) fn option_row(key: &str, title: &str, description: &str, options: El) -> El {
    row()
        .key(format!("{key}-row"))
        .gap(24.0)
        .py(20.0)
        .center_y()
        .flex_wrap()
        .w_full()
        .min_w(0.0)
        .child(
            col()
                .w(0.0)
                .min_w(200.0)
                .grow(1.0)
                .gap(6.0)
                .child(
                    text(title)
                        .key(format!("{key}-label"))
                        .wrap()
                        .font(type_::ITEM_TITLE)
                        .color(tokens::theme().text),
                )
                .child(label(description).key(format!("{key}-description"))),
        )
        .child(
            options
                .key(format!("{key}-choices"))
                .w(0.0)
                .min_w(280.0)
                .grow(1.0),
        )
}

pub(in crate::ui) fn section_header(title: &str, description: &str) -> El {
    col()
        .w_full()
        .gap(6.0)
        .pb(20.0)
        .child(
            text(title)
                .font(type_::SECTION_TITLE)
                .color(tokens::theme().text),
        )
        .child(label(description))
}
