//! Bind reusable controls to this app's state type and input shortcuts.
use super::{
    State,
    app::on_input_key,
    primitives::{button::Button, input},
};
use rune::{El, Icon, TextEdit};

pub(super) fn action(
    key: impl Into<String>,
    title: impl Into<String>,
    f: impl Fn(&mut State) + 'static,
) -> El {
    Button::new(key, title).on_press(f)
}

pub(super) fn button(
    key: impl Into<String>,
    title: impl Into<String>,
    f: impl Fn(&mut State) + 'static,
) -> El {
    Button::new(key, title).frame().on_press(f)
}

pub(super) fn select_action(
    key: impl Into<String>,
    title: impl Into<String>,
    selected: bool,
    f: impl Fn(&mut State) + 'static,
) -> El {
    Button::new(key, title).selected(selected).on_press(f)
}

pub(super) fn icon_action(
    key: impl Into<String>,
    title: impl Into<String>,
    glyph: &'static Icon,
    f: impl Fn(&mut State) + 'static,
) -> El {
    Button::new(key, title).icon(glyph).on_press(f)
}

pub(super) fn input(
    key: &'static str,
    edit: &TextEdit,
    placeholder: &'static str,
    get: fn(&mut State) -> &mut TextEdit,
    submit: impl Fn(&mut State) + 'static,
) -> El {
    input::input(key, edit, placeholder, get, on_input_key, submit)
}

pub(super) fn input_with_change(
    key: &'static str,
    edit: &TextEdit,
    placeholder: &'static str,
    get: fn(&mut State) -> &mut TextEdit,
    submit: impl Fn(&mut State) + 'static,
    change: impl Fn(&mut State, &mut rune::Tx) + 'static,
) -> El {
    input::input_with_change(key, edit, placeholder, get, on_input_key, submit, change)
}
