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
fn unified_addons_page_and_busy_install_spinner() {
    let mut s = init();
    s.navigation.page = Page::Addons;
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("addons-installed-tab").is_none());
    assert!(rt.find("supported-addon-0").is_some());
    assert!(rt.find("supported-addon-1").is_some());
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
    assert_eq!(install.label.as_deref(), Some("Install, busy"));
    assert!(rt.find("addon-install-spinner").is_some());
    assert!(
        !rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("Working…"))
    );
}

#[test]
fn installed_addons_share_the_available_list_without_duplicate_cards() {
    let mut s = init();
    s.config_busy = false;
    s.navigation.page = Page::Addons;
    s.addons = vec![
        serde_json::from_value(serde_json::json!({
            "manifest_url": crate::services::CINEMETA,
        "manifest": {"id":"com.linvo.cinemeta", "name":"Cinemeta", "catalogs":[{"id":"top", "type":"movie"}]}
        }))
        .unwrap(),
    ];
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.frame(0.0);
    assert!(rt.find("installed-addon-0").is_some());
    assert!(rt.find("supported-addon-0").is_none());
    assert!(rt.find("supported-addon-1").is_some());
    let toggle = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("toggle-addon-0"))
        .unwrap();
    assert_eq!(toggle.label.as_deref(), Some("Disable"));
    assert_eq!(rt.state.catalogs().len(), 1);
    rt.update(|s, _| s.addons[0].disabled = true);
    rt.frame(0.0);
    let toggle = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("toggle-addon-0"))
        .unwrap();
    assert_eq!(toggle.label.as_deref(), Some("Enable"));
    assert!(rt.state.catalogs().is_empty());
}

#[test]
fn install_completion_replaces_the_spinner_with_a_checkmark() {
    let (mut s, tx) = state_with_events();
    s.navigation.page = Page::Addons;
    s.addon_install_open = true;
    s.config_busy = true;
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.poll = Some(poll);
    rt.frame(0.0);
    assert!(rt.find("addon-install-spinner").is_some());
    tx.try_send(Event::Addons(Ok(vec![]))).unwrap();
    rt.frame(1.0 / 60.0);
    assert!(rt.find("addon-install-spinner").is_none());
    assert!(rt.find("addon-install-success").is_some());
    tap(&mut rt, "addon-install-done");
    assert!(!rt.state.addon_install_open);
}

#[test]
fn failed_install_stops_the_spinner_and_restores_retry() {
    let (mut s, tx) = state_with_events();
    s.addon_install_open = true;
    s.config_busy = true;
    let mut rt = rune::Runtime::new(s, view, (1000.0, 800.0), rune::Env::default());
    rt.poll = Some(poll);
    rt.frame(0.0);
    tx.try_send(Event::Addons(Err("Cannot fetch manifest".into())))
        .unwrap();
    rt.frame(1.0 / 60.0);
    assert!(rt.find("addon-install-spinner").is_none());
    assert!(rt.state.addon_install_open);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("Cannot fetch manifest"))
    );
    assert!(
        rt.elements()
            .iter()
            .find(|e| e.key.as_deref() == Some("install-addon"))
            .unwrap()
            .focusable
    );
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
