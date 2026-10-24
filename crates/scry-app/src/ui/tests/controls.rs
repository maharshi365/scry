use super::*;
use crate::ui::{
    design::tokens,
    primitives::{button::Button, input::input},
};

#[test]
fn icon_only_actions_have_action_names_not_glyph_labels() {
    let mut rt = rune::Runtime::new(init(), view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let refresh = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("page-refresh"))
        .unwrap();
    assert_eq!(refresh.label.as_deref(), Some("Refresh current page"));
    assert!(refresh.focusable);
}

#[test]
fn addon_tabs_report_selection_and_busy_install_cannot_activate() {
    let mut s = init();
    s.navigation.page = Page::Addons;
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    tap(&mut rt, "addons-installed-tab");
    let selected = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("addons-installed-tab"))
        .unwrap();
    assert_eq!(selected.label.as_deref(), Some("Installed · 0, selected"));
    rt.update(|s, _| {
        s.addon_install_open = true;
        s.config_busy = true;
    });
    rt.frame(0.0);
    let install = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("install-addon"))
        .unwrap();
    assert!(
        !install.focusable,
        "busy installation must not offer an active control"
    );
    assert_eq!(install.label.as_deref(), Some("Working…, busy"));
}

#[test]
fn button_states_gate_actions_and_share_selection_styling() {
    struct Presses {
        count: u32,
    }
    fn controls(_: &Presses, _: &mut rune::Cx<Presses>) -> rune::El {
        rune::col()
            .child(
                Button::new("enabled", "Choose")
                    .selected(true)
                    .on_press(|n: &mut Presses| n.count += 1),
            )
            .child(
                Button::new("disabled", "Unavailable")
                    .enabled(false)
                    .on_press(|n: &mut Presses| n.count += 10),
            )
            .child(
                Button::new("busy", "Saving")
                    .busy(true)
                    .on_press(|n: &mut Presses| n.count += 100),
            )
    }
    let mut rt = rune::Runtime::new(
        Presses { count: 0 },
        controls,
        (400.0, 300.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let selected = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("enabled"))
        .unwrap();
    assert_eq!(selected.fill, tokens::selected());
    assert_eq!(selected.label.as_deref(), Some("Choose, selected"));
    for name in ["disabled", "busy"] {
        let (x, y) = rt.find(name).unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.count, 0, "{name} must not run its callback");
        assert!(
            !rt.elements()
                .into_iter()
                .find(|e| e.key.as_deref() == Some(name))
                .unwrap()
                .focusable
        );
    }
    rt.key(rune::KeyEvent {
        key: rune::Key::Tab,
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert!(
        rt.elements()
            .into_iter()
            .any(|e| e.key.as_deref() == Some("enabled") && e.focused)
    );
    rt.key(rune::KeyEvent {
        key: rune::Key::Enter,
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert_eq!(
        rt.state.count, 1,
        "keyboard activation must reach the supplied callback"
    );
}

#[test]
fn input_submission_uses_the_callers_callback_not_its_key() {
    struct Form {
        edit: TextEdit,
        submitted: String,
    }
    fn form(s: &Form, _: &mut rune::Cx<Form>) -> rune::El {
        input(
            "arbitrary-field",
            &s.edit,
            "Value",
            |s: &mut Form| &mut s.edit,
            |_, _, _| false,
            |s| s.submitted.clone_from(&s.edit.text),
        )
    }
    let mut rt = rune::Runtime::new(
        Form {
            edit: TextEdit::new("A value"),
            submitted: String::new(),
        },
        form,
        (400.0, 300.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let (x, y) = rt.find("arbitrary-field").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.key(rune::KeyEvent {
        key: rune::Key::Enter,
        mods: rune::Mods::default(),
    });
    rt.frame(0.0);
    assert_eq!(rt.state.submitted, "A value");
}
