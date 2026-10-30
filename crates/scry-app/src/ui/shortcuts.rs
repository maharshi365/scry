//! Shared shortcut routing. Features own bindings and actions; the app registers handlers.
use super::State;
use rune::{Key, KeyEvent};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Focus {
    Screen,
    TextInput,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    Local,
    Screen,
    App,
}

pub(super) struct Registration {
    pub scope: Scope,
    pub handle: fn(&mut State, &KeyEvent, Focus) -> bool,
}

pub(super) enum KeyMatch {
    Text(&'static str),
    Left,
    Right,
    Up,
    Down,
    Escape,
    Function(u8),
}

pub(super) enum Modifiers {
    Plain,
    Alt,
    Any,
}

pub(super) struct Binding<A> {
    pub key: KeyMatch,
    pub modifiers: Modifiers,
    pub in_text: bool,
    pub action: A,
}

pub(super) fn resolve<A: Copy>(
    bindings: &[Binding<A>],
    event: &KeyEvent,
    focus: Focus,
) -> Option<A> {
    bindings.iter().find_map(|binding| {
        if focus == Focus::TextInput && !binding.in_text {
            return None;
        }
        let mods = event.mods;
        let modifiers_match = match binding.modifiers {
            Modifiers::Plain => !mods.alt && !mods.ctrl && !mods.cmd && !mods.logo,
            Modifiers::Alt => mods.alt,
            Modifiers::Any => true,
        };
        let key_matches = match (&binding.key, &event.key) {
            (KeyMatch::Text(expected), Key::Text(actual)) => actual.eq_ignore_ascii_case(expected),
            (KeyMatch::Function(expected), Key::Function(actual)) => expected == actual,
            (KeyMatch::Left, Key::Left)
            | (KeyMatch::Right, Key::Right)
            | (KeyMatch::Up, Key::Up)
            | (KeyMatch::Down, Key::Down)
            | (KeyMatch::Escape, Key::Escape) => true,
            _ => false,
        };
        (modifiers_match && key_matches).then_some(binding.action)
    })
}

/// Scope takes priority over registration order. The first handled key stops dispatch.
pub(super) fn dispatch(
    state: &mut State,
    event: &KeyEvent,
    focus: Focus,
    registrations: &[Registration],
) -> bool {
    for scope in [Scope::Local, Scope::Screen, Scope::App] {
        for registration in registrations.iter().filter(|entry| entry.scope == scope) {
            if (registration.handle)(state, event, focus) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::app::init;

    fn key() -> KeyEvent {
        KeyEvent {
            key: Key::Text("k".into()),
            mods: rune::Mods::default(),
        }
    }

    fn app(s: &mut State, _: &KeyEvent, _: Focus) -> bool {
        s.player.status.push_str("app");
        true
    }

    fn screen(s: &mut State, _: &KeyEvent, _: Focus) -> bool {
        s.player.status.push_str("screen");
        true
    }

    fn local(s: &mut State, _: &KeyEvent, _: Focus) -> bool {
        s.player.status.push_str("local");
        true
    }

    #[test]
    fn nearest_scope_wins_even_when_registered_last() {
        let mut state = init();
        let registrations = [
            Registration {
                scope: Scope::App,
                handle: app,
            },
            Registration {
                scope: Scope::Screen,
                handle: screen,
            },
            Registration {
                scope: Scope::Local,
                handle: local,
            },
        ];
        assert!(dispatch(&mut state, &key(), Focus::Screen, &registrations));
        assert_eq!(state.player.status, "local");
        state.player.status.clear();
        assert!(dispatch(
            &mut state,
            &key(),
            Focus::Screen,
            &registrations[..2]
        ));
        assert_eq!(state.player.status, "screen");
    }

    #[test]
    fn unhandled_scopes_fall_through_and_unknown_keys_remain_unhandled() {
        fn only_k(s: &mut State, event: &KeyEvent, _: Focus) -> bool {
            if event.key != Key::Text("k".into()) {
                return false;
            }
            s.player.status.push_str("app");
            true
        }
        let mut state = init();
        let registrations = [
            Registration {
                scope: Scope::Local,
                handle: |_, _, _| false,
            },
            Registration {
                scope: Scope::Screen,
                handle: |_, _, _| false,
            },
            Registration {
                scope: Scope::App,
                handle: only_k,
            },
        ];
        assert!(dispatch(&mut state, &key(), Focus::Screen, &registrations));
        assert_eq!(state.player.status, "app");
        state.player.status.clear();
        let unknown = KeyEvent {
            key: Key::Enter,
            mods: rune::Mods::default(),
        };
        assert!(!dispatch(
            &mut state,
            &unknown,
            Focus::Screen,
            &registrations
        ));
        assert!(state.player.status.is_empty());
    }

    #[test]
    fn text_input_requires_explicit_binding_permission() {
        let mut binding = Binding {
            key: KeyMatch::Text("k"),
            modifiers: Modifiers::Plain,
            in_text: false,
            action: 42,
        };
        assert_eq!(
            resolve(std::slice::from_ref(&binding), &key(), Focus::TextInput),
            None
        );
        binding.in_text = true;
        assert_eq!(resolve(&[binding], &key(), Focus::TextInput), Some(42));
    }

    #[test]
    fn plain_bindings_accept_shifted_letters_but_not_command_modifiers() {
        let bindings = [Binding {
            key: KeyMatch::Text("k"),
            modifiers: Modifiers::Plain,
            in_text: false,
            action: 42,
        }];
        let shifted = KeyEvent {
            key: Key::Text("K".into()),
            mods: rune::Mods {
                shift: true,
                ..rune::Mods::default()
            },
        };
        assert_eq!(resolve(&bindings, &shifted, Focus::Screen), Some(42));
        for mods in [
            rune::Mods {
                ctrl: true,
                ..rune::Mods::default()
            },
            rune::Mods {
                alt: true,
                ..rune::Mods::default()
            },
            rune::Mods {
                cmd: true,
                ..rune::Mods::default()
            },
            rune::Mods {
                logo: true,
                ..rune::Mods::default()
            },
        ] {
            let event = KeyEvent {
                key: Key::Text("k".into()),
                mods,
            };
            assert_eq!(resolve(&bindings, &event, Focus::Screen), None);
        }
    }
}
