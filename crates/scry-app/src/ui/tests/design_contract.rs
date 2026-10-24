//! Keep visual values in the design layer. This checks our source policy, not Rune.
use std::path::Path;

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
                ]
                .iter()
                .any(|pattern| text.contains(pattern))
            };
            let inline_svg = !is_icons && text.contains("body: r#");
            let feature_dependency = parent == "primitives"
                && [
                    "State",
                    "crate::services",
                    "crate::ui::player",
                    "crate::ui::navigation",
                ]
                .iter()
                .any(|pattern| text.contains(pattern));
            if forbidden || inline_svg || feature_dependency {
                violations.push(format!("{}:{}: {}", path.display(), line + 1, text.trim()));
            }
        }
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
