//! Navigation bindings do not depend on the player shortcut handler.
use super::super::{
    Page, State,
    shortcuts::{Binding, Focus, KeyMatch, Modifiers, resolve},
};
use rune::KeyEvent;

const HISTORY: &[Binding<bool>] = &[
    Binding {
        key: KeyMatch::Left,
        modifiers: Modifiers::Alt,
        in_text: true,
        action: false,
    },
    Binding {
        key: KeyMatch::Right,
        modifiers: Modifiers::Alt,
        in_text: true,
        action: true,
    },
];

const BACK: &[Binding<bool>] = &[Binding {
    key: KeyMatch::Escape,
    modifiers: Modifiers::Any,
    in_text: false,
    action: false,
}];

pub(in crate::ui) fn history(s: &mut State, event: &KeyEvent, focus: Focus) -> bool {
    let Some(forward) = resolve(HISTORY, event, focus) else {
        return false;
    };
    s.travel(forward);
    true
}

pub(in crate::ui) fn screen(s: &mut State, event: &KeyEvent, focus: Focus) -> bool {
    if !matches!(s.navigation.page, Page::Detail | Page::Player) {
        return false;
    }
    let Some(forward) = resolve(BACK, event, focus) else {
        return false;
    };
    s.travel(forward);
    true
}
