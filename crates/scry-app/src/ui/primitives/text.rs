use super::super::design::tokens::{theme, type_};
use rune::{El, text};

pub(in crate::ui) fn label(value: impl Into<String>) -> El {
    Variant::Body.render(value)
}

pub(in crate::ui) fn heading(value: impl Into<String>) -> El {
    Variant::Heading.render(value)
}

#[derive(Clone, Copy)]
pub(in crate::ui) enum Variant {
    Body,
    Heading,
    Caption,
}

impl Variant {
    pub fn render(self, value: impl Into<String>) -> El {
        let (font, color) = match self {
            Self::Body => (type_::BODY, theme().text_secondary),
            Self::Heading => (type_::HEADING, theme().text),
            Self::Caption => (type_::CAPTION, theme().text_secondary),
        };
        text(value).font(font).wrap().color(color)
    }
}
