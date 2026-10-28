//! Keep visual values in the design layer. This checks our source policy, not Rune.
use std::path::Path;

fn raw_visual_value(text: &str) -> bool {
    [".radius(", ".opacity(", ".border("].iter().any(|method| {
        text.split(method)
            .skip(1)
            .any(|value| value.trim_start().starts_with(|c: char| c.is_ascii_digit()))
    })
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
            if forbidden || inline_svg || inline_focus || inline_control || feature_dependency {
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
