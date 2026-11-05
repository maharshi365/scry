use anyhow::{Context, Result, ensure};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::LazyLock,
};
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

#[cfg(test)]
mod tests;

#[derive(Deserialize, Serialize)]
struct Settings {
    retention_days: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self { retention_days: 7 }
    }
}

fn load_settings(data: &Path) -> Result<Settings> {
    let path = data.join("logging-settings.json");
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let settings = Settings::default();
            crate::storage::atomic_write(&path, &serde_json::to_vec(&settings)?)?;
            return Ok(settings);
        }
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 4096, "Logging settings are too large");
    let settings: Settings = serde_json::from_slice(&bytes)?;
    ensure!(
        settings.retention_days > 0,
        "Log retention must be at least one day"
    );
    Ok(settings)
}

pub(crate) fn init() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let data = scry_app::project_dir(false).context("Cannot locate application log folder")?;
    let folder = data.join("logs");
    fs::create_dir_all(&folder).context("Cannot create application log folder")?;
    // Invalid settings must not prevent playback or leave the app without logs.
    let settings = load_settings(&data);
    let retention = settings
        .as_ref()
        .map_or(7, |settings| settings.retention_days);
    let mut daily = DailyWriter::new(folder, retention);
    daily
        .write_on(Utc::now().date_naive(), b"")
        .context("Cannot open daily application log")?;
    let (writer, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .lossy(true)
        .finish(daily);
    subscriber(writer)
        .try_init()
        .context("Cannot initialize application logging")?;
    if let Err(error) = settings {
        tracing::warn!(reason = %error_detail(&error), "Invalid logging settings; using 7-day retention");
    }
    // Panic payloads may include URLs or credentials. Record only their source location.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        if let Some(location) = panic.location() {
            tracing::error!(
                file = location.file(),
                line = location.line(),
                "Application panicked"
            );
        } else {
            tracing::error!("Application panicked");
        }
        previous(panic);
    }));
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        retention_days = retention,
        "Application started"
    );
    Ok(guard)
}

fn subscriber(
    writer: tracing_appender::non_blocking::NonBlocking,
) -> impl tracing::Subscriber + Send + Sync {
    tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(writer)
            .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                // Dependencies can emit private tracker URLs, headers, and source details.
                matches!(
                    metadata.target().split("::").next(),
                    Some(
                        "scry"
                            | "scry_app"
                            | "scry_library"
                            | "scry_playback"
                            | "scry_torrents"
                            | "scry_addons"
                    )
                ) && *metadata.level() <= tracing::Level::INFO
            })),
    )
}

pub(crate) fn error_detail(error: &anyhow::Error) -> String {
    redact(&format!("{error:#}"))
}

pub(crate) fn redact(text: &str) -> String {
    static PRIVATE: LazyLock<Option<regex::Regex>> = LazyLock::new(|| {
        regex::Regex::new(r#"(?i)(?:[a-z][a-z0-9+.-]*://|magnet:)[^\s<>]+|(?:authorization|cookie|password|token|api[_-]?key)["']?\s*[:=][^\r\n]+"#).ok()
    });
    let Some(pattern) = PRIVATE.as_ref() else {
        return "[error details unavailable]".into();
    };
    // Stream into the capped output instead of allocating a redacted copy of
    // the entire input, which can include a large response or error body.
    let mut output = String::with_capacity(text.len().min(4096));
    let mut remaining = 4096;
    let mut append = |part: &str| {
        for c in part.chars().take(remaining) {
            output.push(if c.is_control() { ' ' } else { c });
            remaining -= 1;
        }
        remaining != 0
    };
    for (index, part) in pattern.split(text).enumerate() {
        if (index != 0 && !append("[redacted]")) || !append(part) {
            break;
        }
    }
    output
}

pub(crate) fn playback_stopped(attempt: u64, error: Option<&str>) {
    if let Some(reason) = error {
        tracing::warn!(attempt, reason = %redact(reason), "Playback stopped; Choose another source to continue");
    } else {
        tracing::info!(
            attempt,
            "Playback stopped without an error; Choose another source to continue"
        );
    }
}

struct DailyWriter {
    folder: PathBuf,
    retention: u16,
    current: Option<(NaiveDate, File)>,
}

impl DailyWriter {
    fn new(folder: PathBuf, retention: u16) -> Self {
        Self {
            folder,
            retention,
            current: None,
        }
    }

    fn write_on(&mut self, date: NaiveDate, bytes: &[u8]) -> io::Result<()> {
        if self.current.as_ref().is_none_or(|(day, _)| *day != date) {
            // Close yesterday's file before pruning it (required on Windows).
            self.current = None;
            let cleaned = cleanup(&self.folder, date, self.retention);
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.folder.join(format!("scry-{date}.log")))?;
            if let Err(error) = cleaned {
                // Do not trace from the log worker: its own queue may be full.
                writeln!(
                    file,
                    "{} WARN Log retention cleanup failed: {}",
                    Utc::now().to_rfc3339(),
                    redact(&error.to_string())
                )?;
            }
            self.current = Some((date, file));
        }
        if let Some((_, file)) = &mut self.current {
            file.write_all(bytes)?;
        }
        Ok(())
    }
}

impl Write for DailyWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write_on(Utc::now().date_naive(), bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some((_, file)) = &mut self.current {
            file.flush()?;
        }
        Ok(())
    }
}

fn cleanup(folder: &Path, today: NaiveDate, retention: u16) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(date) = name
            .strip_prefix("scry-")
            .and_then(|name| name.strip_suffix(".log"))
            .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        else {
            continue;
        };
        if today.signed_duration_since(date).num_days() >= i64::from(retention) {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}
