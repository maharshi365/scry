use super::super::design::tokens::{theme, type_};
use rune::{El, text};

pub(in crate::ui) fn label(value: impl Into<String>) -> El {
    text(value)
        .font(type_::BODY)
        .wrap()
        .color(theme().text_secondary)
}

pub(in crate::ui) fn heading(value: impl Into<String>) -> El {
    text(value).wrap().font(type_::HEADING).color(theme().text)
}
