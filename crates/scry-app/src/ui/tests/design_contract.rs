//! Keep visual values in the design layer. This checks our source policy, not Rune.
use std::path::Path;

fn raw_visual_value(text: &str) -> bool {
    [".radius(", ".opacity(", ".border("].iter().any(|method| {
        text.split(method)
            .skip(1)
            .any(|value| value.trim_start().starts_with(|c: char| c.is_ascii_digit()))
    })
}

#[test]
fn native_drag_regions_stay_in_the_shared_titlebar() {
    let shell = include_str!("../shell.rs");
    assert!(!shell.contains(".window_drag()"));
    assert!(shell.contains("primitives::titlebar::titlebar(native_inset)"));
    let primitive = include_str!("../primitives/titlebar.rs");
    assert!(primitive.contains(".window_drag()"));
}

#[test]
fn intro_prompt_reuses_shared_actions_feedback_and_motion() {
    let source = include_str!("../player/intro.rs");
    assert!(source.contains("Button::new(\"player-skip-intro\""));
    assert!(source.contains("feedback::status("));
    assert!(source.contains("primitives::motion::fade()"));
    assert!(source.contains("tokens::player::INTRO_FULLSCREEN_BOTTOM"));
    assert!(!source.contains(".fill("));
}

#[test]
fn full_page_empty_states_are_not_constrained_by_virtual_poster_rows() {
    let grid = include_str!("../poster_grid.rs");
    assert!(
        !grid.contains("watching_empty("),
        "The page owns its empty state; virtual poster rows must not size it"
    );
}

fn local_motion(text: &str) -> bool {
    ["Transition::", "Spring::", "Anim::", ".hover_motion("]
        .iter()
        .any(|pattern| text.contains(pattern))
}

fn check_source(path: &Path, violations: &mut Vec<String>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().unwrap() != "tests" {
                check_source(&path, violations);
            }
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let parent = path.parent().unwrap().file_name().unwrap();
        let is_tokens = parent == "design" && path.file_name().unwrap() == "tokens.rs";
        let is_icons = parent == "design" && path.file_name().unwrap() == "icons.rs";
        if !is_tokens && raw_visual_value(&source) {
            violations.push(format!(
                "{}: raw visual value outside tokens",
                path.display()
            ));
        }
        for (line, text) in source.lines().enumerate() {
            let forbidden = if is_tokens {
                false
            } else {
                [
                    "oklch(",
                    "oklcha(",
                    "color::hex(",
                    "tok::theme(",
                    "TextStyle::new(",
                    ".alpha(",
                    "Color::",
                    "rune::rgb(",
                    "rune::rgba(",
                ]
                .iter()
                .any(|pattern| text.contains(pattern))
            };
            let inline_svg = !is_icons && text.contains("body: r#");
            let inline_focus = parent != "primitives"
                && [".focus_ring(", ".radius(", ".border("]
                    .iter()
                    .any(|pattern| text.contains(pattern));
            let inline_control = parent != "primitives"
                && [
                    ".role(\"button\")",
                    ".role(\"progressbar\")",
                    ".role(\"slider\")",
                ]
                .iter()
                .any(|pattern| text.contains(pattern));
            let feature_dependency = parent == "primitives"
                && [
                    "State",
                    "crate::services",
                    "crate::ui::player",
                    "crate::ui::navigation",
                ]
                .iter()
                .any(|pattern| text.contains(pattern));
            let local_motion = !is_tokens && parent != "primitives" && local_motion(text);
            if forbidden
                || inline_svg
                || inline_focus
                || inline_control
                || feature_dependency
                || local_motion
            {
                violations.push(format!("{}:{}: {}", path.display(), line + 1, text.trim()));
            }
        }
    }
}

#[test]
fn visual_policy_detects_literals_but_allows_shared_tokens() {
    for source in [
        "el().radius(8.0)",
        ".opacity( 0.5)",
        ".border(1.0, edge())",
        ".border(\n  1.0, edge())",
    ] {
        assert!(raw_visual_value(source), "{source}");
    }
    for source in [
        ".radius(tokens::radius::SM)",
        ".border(tokens::border::WIDTH, edge())",
    ] {
        assert!(!raw_visual_value(source), "{source}");
    }
}

#[test]
fn motion_policy_rejects_local_tuning_but_allows_shared_intent() {
    for source in [
        "rune::Transition::opacity()",
        "use rune::Transition; Transition::scale(0.96)",
        ".hover_motion(Anim::INSTANT, Anim::INSTANT)",
        "Spring::new(0.3, 0.0)",
    ] {
        assert!(local_motion(source), "{source}");
    }
    for source in [
        ".transition(primitives::motion::fade())",
        "motion::surface(panel)",
        "motion::playback_feedback(elapsed)",
    ] {
        assert!(!local_motion(source), "{source}");
    }
}

#[test]
fn compact_icons_follow_small_text_without_shrinking_the_click_target() {
    use super::super::{
        design::{icons, tokens::type_},
        primitives::button::{Button, Size},
    };
    struct Policy {
        presses: usize,
    }
    fn view(_: &Policy, _: &mut rune::Cx<Policy>) -> rune::El {
        rune::col().child(
            Button::new("compact", "Expand")
                .size(Size::Compact)
                .icon(&icons::EXPAND)
                .on_press(|s: &mut Policy| s.presses += 1),
        )
    }
    let mut rt = rune::Runtime::new(
        Policy { presses: 0 },
        view,
        (200.0, 100.0),
        rune::Env::default(),
    );
    rt.frame(0.0);
    let elements = rt.elements();
    let index = elements
        .iter()
        .position(|e| e.key.as_deref() == Some("compact"))
        .unwrap();
    let glyph = elements.iter().find(|e| e.parent == Some(index)).unwrap();
    assert_eq!(glyph.rect[3], type_::SMALL.size * 1.2);
    assert!(elements[index].rect[3] >= glyph.rect[3] * 2.0);
    let target = elements[index].rect;
    rt.pointer_down(target[0] + 2.0, target[1] + 2.0);
    rt.pointer_up(target[0] + 2.0, target[1] + 2.0);
    rt.frame(0.0);
    assert_eq!(
        rt.state.presses, 1,
        "The area outside the small glyph must remain clickable"
    );
}

#[test]
fn ui_uses_shared_visual_values_and_feature_free_primitives() {
    let mut violations = Vec::new();
    check_source(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui"),
        &mut violations,
    );
    assert!(
        violations.is_empty(),
        "UI design policy violations:\n{}",
        violations.join("\n")
    );
}
