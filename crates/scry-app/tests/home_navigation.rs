use std::{fs, process::Command};

#[test]
fn homepage_movie_opens_sources_on_the_application_stack() -> anyhow::Result<()> {
    let tree = render_scene(
        "watching",
        r#"[{"t":0.5,"key":"watching-open-0","type":"tap"}]"#,
    )?;
    assert!(tree.contains("\"detail-page\""), "Movie details must open");
    assert!(
        tree.contains("\"source-picker\""),
        "Sources must be reachable"
    );
    Ok(())
}

#[test]
fn populated_movie_sources_fit_the_application_stack() -> anyhow::Result<()> {
    let tree = render_scene("source-wrap", "[]")?;
    assert!(
        tree.contains("\"stream-release-0\""),
        "Source details must render"
    );
    assert!(
        tree.contains("\"stream-play-0\""),
        "Source actions must render"
    );
    Ok(())
}

fn render_scene(scene: &str, events: &str) -> anyhow::Result<String> {
    let directory = tempfile::tempdir()?;
    let script = directory.path().join("click.json");
    let tree = directory.path().join("tree.json");
    fs::write(&script, events)?;
    // A subprocess uses the application's main stack, not the larger test-thread stack.
    let output = Command::new(env!("CARGO_BIN_EXE_scry"))
        .args(["--scene", scene, "--script"])
        .arg(&script)
        .arg("--tree")
        .arg(&tree)
        .args(["--at", "0.6"])
        .env("SCRY_DATA_ROOT", directory.path().join("data"))
        .output()?;
    assert!(
        output.status.success(),
        "Scene {scene} must not crash: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(fs::read_to_string(tree)?)
}
