use super::super::design::{icons, tokens};
use super::{button::Button, surface::card, text::heading};
use rune::{El, el, row};

/// Full-window scrim. The caller supplies the modal panel and its contents.
pub(in crate::ui) fn layer(key: impl Into<String>, content: El) -> El {
    el().key(key)
        .w_full()
        .h_full()
        .absolute(0.0, 0.0)
        .center()
        .fill(tokens::scrim())
        .child(content)
}

pub(in crate::ui) fn panel<S: 'static>(
    key: impl Into<String>,
    dismiss: impl Fn(&mut S) + 'static,
) -> El {
    card().key(key).modal(true, dismiss)
}

pub(in crate::ui) fn header(title: impl Into<String>) -> El {
    row()
        .w_full()
        .center_y()
        .gap(12.0)
        .child(heading(title).w(0.0).grow(1.0).min_w(0.0))
}

pub(in crate::ui) fn close<S: 'static>(
    key: impl Into<String>,
    title: impl Into<String>,
    dismiss: impl Fn(&mut S) + 'static,
) -> El {
    Button::new(key, title)
        .icon(&icons::CLOSE)
        .on_press(dismiss)
}
