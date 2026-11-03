use std::{fs, process::Command};

#[test]
fn startup_prunes_expired_logs_and_flushes_failures_before_exit() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let data = directory.path().join("data");
    let logs = data.join("logs");
    fs::create_dir_all(&logs)?;
    fs::write(
        data.join("logging-settings.json"),
        r#"{"retention_days":2}"#,
    )?;
    fs::write(logs.join("scry-2000-01-01.log"), "expired")?;
    fs::write(logs.join("unrelated.log"), "keep")?;

    // A missing report fails without a window, network request, or native playback runtime.
    let output = Command::new(env!("CARGO_BIN_EXE_scry"))
        .arg("--analyze")
        .arg(directory.path().join("missing-report.jsonl"))
        .env("SCRY_DATA_ROOT", directory.path())
        .env_remove("SCRY_PROFILE_ROOT")
        .output()?;
    assert!(!output.status.success(), "The missing report must fail");
    assert!(!logs.join("scry-2000-01-01.log").exists());
    assert_eq!(fs::read_to_string(logs.join("unrelated.log"))?, "keep");
    let path = logs.join(format!("scry-{}.log", chrono::Utc::now().date_naive()));
    let text = fs::read_to_string(path)?;
    assert!(text.contains("Application started"));
    assert!(text.contains("retention_days=2"));
    assert!(text.contains("Application failed"));
    assert!(text.contains("Application shutting down"));
    assert!(text.contains("success=false"));
    Ok(())
}
