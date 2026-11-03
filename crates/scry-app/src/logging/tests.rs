use super::*;

#[test]
fn retention_removes_only_expired_daily_logs() -> Result<()> {
    let root = tempfile::tempdir()?;
    let today = NaiveDate::from_ymd_opt(2026, 11, 2).context("invalid fixture date")?;
    for name in [
        "scry-2026-10-26.log",
        "scry-2026-10-27.log",
        "scry-2026-11-02.log",
        "notes.log",
        "scry-invalid.log",
    ] {
        fs::write(root.path().join(name), "existing\n")?;
    }
    cleanup(root.path(), today, 7)?;
    assert!(!root.path().join("scry-2026-10-26.log").exists());
    for name in [
        "scry-2026-10-27.log",
        "scry-2026-11-02.log",
        "notes.log",
        "scry-invalid.log",
    ] {
        assert_eq!(fs::read_to_string(root.path().join(name))?, "existing\n");
    }
    Ok(())
}

#[test]
fn rotation_appends_and_prunes_without_restart() -> Result<()> {
    let root = tempfile::tempdir()?;
    let first = NaiveDate::from_ymd_opt(2026, 11, 1).context("invalid fixture date")?;
    let next = first.succ_opt().context("invalid next day")?;
    fs::write(root.path().join("scry-2026-11-01.log"), "previous\n")?;
    let mut writer = DailyWriter::new(root.path().to_owned(), 1);
    writer.write_on(first, b"first\n")?;
    writer.flush()?;
    assert_eq!(
        fs::read_to_string(root.path().join("scry-2026-11-01.log"))?,
        "previous\nfirst\n"
    );
    writer.write_on(next, b"next\n")?;
    writer.flush()?;
    assert!(!root.path().join("scry-2026-11-01.log").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("scry-2026-11-02.log"))?,
        "next\n"
    );
    Ok(())
}

#[test]
fn retention_settings_are_persisted_and_invalid_values_rejected() -> Result<()> {
    let root = tempfile::tempdir()?;
    assert_eq!(load_settings(root.path())?.retention_days, 7);
    let path = root.path().join("logging-settings.json");
    assert!(path.is_file());
    fs::write(&path, r#"{"retention_days":14}"#)?;
    assert_eq!(load_settings(root.path())?.retention_days, 14);
    for text in [
        r#"{"retention_days":0}"#,
        r#"{"retention_days":-1}"#,
        "invalid",
    ] {
        fs::write(&path, text)?;
        assert!(load_settings(root.path()).is_err());
    }
    Ok(())
}

#[test]
fn error_details_remove_private_sources_and_control_characters() {
    let error = anyhow::anyhow!(
        "request failed https://user:secret@tracker.test/passkey?q=token\ncaused by magnet:?xt=private\u{1b}[31m"
    );
    let detail = error_detail(&error);
    assert!(detail.contains("request failed"));
    assert!(detail.contains("caused by"));
    for secret in [
        "user",
        "secret",
        "tracker.test",
        "passkey",
        "token",
        "private",
        "\n",
        "\u{1b}",
    ] {
        assert!(!detail.contains(secret), "leaked {secret}");
    }
}

#[test]
fn stopped_event_records_attempt_and_reason_in_file() -> Result<()> {
    let root = tempfile::tempdir()?;
    let writer = DailyWriter::new(root.path().to_owned(), 7);
    let (writer, guard) = tracing_appender::non_blocking(writer);
    let subscriber = subscriber(writer);
    tracing::subscriber::with_default(subscriber, || {
        playback_stopped(42, Some("metadata timed out"));
        tracing::error!(target: "librqbit", "private dependency details");
        tracing::debug!("verbose details");
    });
    drop(guard);
    let text = fs::read_to_string(
        root.path()
            .join(format!("scry-{}.log", Utc::now().date_naive())),
    )?;
    assert!(text.contains("attempt=42"));
    assert!(text.contains("metadata timed out"));
    assert!(text.contains("Choose another source to continue"));
    assert!(!text.contains("private dependency details"));
    assert!(!text.contains("verbose details"));
    Ok(())
}

#[test]
fn credentials_in_error_headers_are_redacted() {
    let text = redact(
        "request failed: Authorization: Bearer secret-value\nCookie: session=secret-cookie\npassword=secret password with spaces",
    );
    assert!(text.contains("request failed"));
    for secret in ["secret-value", "secret-cookie", "secret password"] {
        assert!(!text.contains(secret));
    }
    let text = redact(r#"response failed: {"token":"json-secret"}"#);
    assert!(text.contains("response failed"));
    assert!(!text.contains("json-secret"));
}

#[cfg(windows)]
#[test]
fn locked_expired_log_does_not_prevent_new_logs() -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;

    let root = tempfile::tempdir()?;
    let today = NaiveDate::from_ymd_opt(2026, 11, 2).context("invalid fixture date")?;
    let expired = root.path().join("scry-2026-10-01.log");
    fs::write(&expired, "old")?;
    let _locked = OpenOptions::new().read(true).share_mode(0).open(expired)?;
    let mut writer = DailyWriter::new(root.path().to_owned(), 7);
    writer.write_on(today, b"new event\n")?;
    writer.flush()?;
    let text = fs::read_to_string(root.path().join("scry-2026-11-02.log"))?;
    assert!(text.contains("Log retention cleanup failed"));
    assert!(text.contains("new event"));
    Ok(())
}
