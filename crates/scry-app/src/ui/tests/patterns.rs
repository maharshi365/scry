use super::super::primitives::{
    button::Button,
    feedback::{self, Tone},
    form::option_row,
    pattern,
};

#[derive(Default)]
struct Example {
    presses: usize,
}

fn example(_: &Example, _: &mut rune::Cx<Example>) -> rune::El {
    pattern::page(
        "page",
        "Preferences",
        "Shared layout",
        rune::row().child(Button::new("save", "Save").on_press(|s: &mut Example| s.presses += 1)),
        pattern::section(
            "section",
            "Playback",
            "Used for new playback",
            pattern::settings_group(
                "settings",
                [
                    option_row(
                        "first",
                        "Language",
                        "A description that can wrap on a narrow screen",
                        rune::row().child(
                            Button::new("choose", "English")
                                .on_press(|s: &mut Example| s.presses += 1),
                        ),
                    ),
                    option_row(
                        "second",
                        "Subtitles",
                        "Show translated text",
                        rune::row().child(
                            Button::new("toggle", "Enabled")
                                .on_press(|s: &mut Example| s.presses += 1),
                        ),
                    ),
                ],
            ),
        ),
    )
}

fn bounds(rt: &rune::Runtime<Example>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap()
        .rect
}

#[test]
fn shared_page_keeps_actions_and_rows_inside_the_viewport() {
    for width in [400.0, 880.0, 1280.0] {
        let mut rt = rune::Runtime::new(
            Example::default(),
            example,
            (width, 820.0),
            rune::Env::default(),
        );
        rt.frame(0.0);
        for key in ["save", "first-row", "second-row", "choose", "toggle"] {
            let rect = bounds(&rt, key);
            assert!(rect[2] > 0.0 && rect[3] > 0.0, "{key}: {rect:?}");
            assert!(
                rect[0] >= 0.0 && rect[0] + rect[2] <= width + 0.5,
                "{width}: {key}: {rect:?}"
            );
        }
        assert!(bounds(&rt, "second-row")[1] > bounds(&rt, "first-row")[1]);
        let (x, y) = rt.find("choose").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.presses, 1);
    }
}

#[test]
fn settings_groups_own_separators_without_an_extra_one_at_either_end() {
    let mut rt = rune::Runtime::new(
        Example::default(),
        example,
        (880.0, 820.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let separators: Vec<_> = rt
        .elements()
        .into_iter()
        .filter(|e| {
            e.key
                .as_deref()
                .is_some_and(|key| key.starts_with("settings-separator-"))
        })
        .collect();
    assert_eq!(separators.len(), 1);
    let line = separators[0].rect;
    let first = bounds(&rt, "first-row");
    let second = bounds(&rt, "second-row");
    assert!(line[1] >= first[1] + first[3] - 0.5);
    assert!(line[1] + line[3] <= second[1] + 0.5);
}

#[test]
fn only_the_control_reacts_to_hover_and_it_reacts_without_a_fade() {
    let mut rt = rune::Runtime::new(
        Example::default(),
        example,
        (880.0, 820.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let (x, y) = rt.find("choose").unwrap();
    rt.pointer_move(x, y);
    rt.frame(0.0);
    let elements = rt.elements();
    let button = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("choose"))
        .unwrap();
    assert_eq!(
        button.hover, 1.0,
        "Control hover must not wait for animation frames"
    );
    let row = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("first-row"))
        .unwrap();
    assert_eq!(
        row.hover, 0.0,
        "The passive settings row must not react with its control"
    );
    rt.pointer_move(0.0, 819.0);
    rt.frame(0.0);
    let elements = rt.elements();
    assert_eq!(
        elements
            .iter()
            .find(|e| e.key.as_deref() == Some("choose"))
            .unwrap()
            .hover,
        0.0
    );
}

#[test]
fn feedback_status_is_not_an_action_and_recovery_is_clickable() {
    fn view(_: &Example, _: &mut rune::Cx<Example>) -> rune::El {
        rune::col()
            .child(feedback::status("ready", "Ready", Tone::Success))
            .child(feedback::message(
                "error",
                "Cannot load",
                "Try again",
                Tone::Danger,
                rune::row().child(
                    Button::new("retry", "Retry").on_press(|s: &mut Example| s.presses += 1),
                ),
            ))
    }
    let mut rt = rune::Runtime::new(
        Example::default(),
        view,
        (600.0, 400.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let elements = rt.elements();
    let status = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("ready"))
        .unwrap();
    assert_eq!(status.role, "status");
    assert!(!status.focusable);
    let (x, y) = rt.find("retry").unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    assert_eq!(rt.state.presses, 1);
}

#[test]
fn shared_controls_resolve_appearance_from_the_central_theme() {
    use super::super::{design::tokens, primitives::button::Variant};
    fn view(_: &Example, _: &mut rune::Cx<Example>) -> rune::El {
        rune::col()
            .child(
                Button::new("primary", "Save")
                    .variant(Variant::Primary)
                    .on_press(|_: &mut Example| {}),
            )
            .child(Button::new("secondary", "Cancel").on_press(|_: &mut Example| {}))
            .child(
                Button::new("selected", "Selected")
                    .selected(true)
                    .on_press(|_: &mut Example| {}),
            )
            .child(feedback::status("failed", "Cannot save", Tone::Danger))
    }
    let mut rt = rune::Runtime::new(
        Example::default(),
        view,
        (600.0, 400.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let elements = rt.elements();
    for (key, expected) in [
        ("primary", tokens::primary_fill()),
        ("secondary", tokens::panel()),
        ("selected", tokens::selected()),
    ] {
        let element = elements
            .iter()
            .find(|e| e.key.as_deref() == Some(key))
            .unwrap();
        assert_eq!(
            element.fill, expected,
            "{key} fill must come from its semantic token"
        );
        assert_eq!(element.radius, tokens::control::RADIUS);
    }
    let message = elements
        .iter()
        .find(|e| e.key.as_deref() == Some("failed"))
        .unwrap();
    assert_eq!(message.text_color, Some(tokens::danger()));
    let label = elements
        .iter()
        .find(|e| e.text.as_deref() == Some("Save"))
        .unwrap();
    assert_eq!(label.text_color, Some(tokens::primary_text()));
}

#[test]
fn install_dialog_uses_matching_shared_input_and_submit_heights() {
    let mut state = super::init();
    state.addons.install_open = true;
    state.addons.busy = false;
    let mut rt = rune::Runtime::new(state, super::view, (880.0, 600.0), rune::Env::default());
    rt.frame(0.0);
    let input = super::rect(&rt, "manifest-frame");
    let submit = super::rect(&rt, "install-addon");
    assert_eq!(
        input[3],
        super::super::design::tokens::control::DIALOG_HEIGHT
    );
    assert_eq!(submit[3], input[3]);
    assert_eq!(
        submit[2],
        super::super::design::tokens::control::DIALOG_SUBMIT_WIDTH
    );
}
