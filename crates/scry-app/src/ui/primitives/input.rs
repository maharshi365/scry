use super::super::design::tokens::{control, edge, panel, theme, type_};
use rune::{El, KeyEvent, TextEdit, Tx, el, text_input};

pub(in crate::ui) fn input<S: 'static>(
    key: &'static str,
    edit: &TextEdit,
    placeholder: &'static str,
    get: fn(&mut S) -> &mut TextEdit,
    on_key: fn(&mut S, &KeyEvent, &mut Tx) -> bool,
    submit: impl Fn(&mut S) + 'static,
) -> El {
    input_with_change(key, edit, placeholder, get, on_key, submit, |_, _| {})
}

pub(in crate::ui) fn input_with_change<S: 'static>(
    key: &'static str,
    edit: &TextEdit,
    placeholder: &'static str,
    get: fn(&mut S) -> &mut TextEdit,
    on_key: fn(&mut S, &KeyEvent, &mut Tx) -> bool,
    submit: impl Fn(&mut S) + 'static,
    change: impl Fn(&mut S, &mut Tx) + 'static,
) -> El {
    let t = theme();
    el().key(format!("{key}-frame"))
        .h(control::INPUT_HEIGHT)
        .min_h(control::INPUT_HEIGHT)
        .grow(1.0)
        .px(12.0)
        .center_y()
        .radius(control::RADIUS)
        .fill(panel())
        .border(1.0, edge())
        .focus_ring(2.0, 2.0, super::super::design::tokens::accent())
        .child(
            text_input(edit, type_::BODY)
                .key(key)
                .w_full()
                .color(t.text)
                .placeholder(placeholder, t.text_secondary)
                .single_line()
                .on_input_key(on_key)
                .on_input_change(change)
                .on_submit(move |s, _| submit(s))
                .on_edit(get),
        )
}
