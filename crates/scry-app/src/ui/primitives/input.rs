use super::super::design::tokens::{self, control, type_};
use rune::{El, KeyEvent, TextEdit, Tx};

pub(in crate::ui) struct FieldText {
    placeholder: &'static str,
    description: Option<&'static str>,
}

impl FieldText {
    pub fn described(placeholder: &'static str, description: &'static str) -> Self {
        Self {
            placeholder,
            description: Some(description),
        }
    }
}

impl From<&'static str> for FieldText {
    fn from(placeholder: &'static str) -> Self {
        Self {
            placeholder,
            description: None,
        }
    }
}

pub(in crate::ui) fn dialog_input<S: 'static>(
    key: &'static str,
    edit: &TextEdit,
    placeholder: impl Into<FieldText>,
    get: fn(&mut S) -> &mut TextEdit,
    submit: impl Fn(&mut S) + 'static,
) -> El {
    input(key, edit, placeholder, get, |_, _, _| false, submit)
        .h(control::DIALOG_HEIGHT)
        .min_h(control::DIALOG_HEIGHT)
}

pub(in crate::ui) fn input<S: 'static>(
    key: &'static str,
    edit: &TextEdit,
    placeholder: impl Into<FieldText>,
    get: fn(&mut S) -> &mut TextEdit,
    on_key: fn(&mut S, &KeyEvent, &mut Tx) -> bool,
    submit: impl Fn(&mut S) + 'static,
) -> El {
    input_with_change(key, edit, placeholder, get, on_key, submit, |_, _| {})
}

pub(in crate::ui) fn input_with_change<S: 'static>(
    key: &'static str,
    edit: &TextEdit,
    placeholder: impl Into<FieldText>,
    get: fn(&mut S) -> &mut TextEdit,
    on_key: fn(&mut S, &KeyEvent, &mut Tx) -> bool,
    submit: impl Fn(&mut S) + 'static,
    change: impl Fn(&mut S, &mut Tx) + 'static,
) -> El {
    let content = placeholder.into();
    let mut input = rune_kit::TextInput::new(edit)
        .placeholder(content.placeholder)
        .dir(rune::Direction::Auto);
    if let Some(description) = content.description {
        input = input.description(description);
    }
    input
        .text_style(type_::BODY)
        .height(control::INPUT_HEIGHT)
        .on_key(on_key)
        .on_change(change)
        .on_submit(move |s, _| submit(s))
        .on_edit(get)
        .build(tokens::kit())
        .key(key)
        .min_h(control::INPUT_HEIGHT)
        .grow(1.0)
}
