#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod artwork;
mod artwork_cache;
mod diagnostics;
mod history_import;
mod library_worker;
mod local_media;
mod logging;
mod media_metadata;
mod search;
mod services;
mod storage;
mod ui;

fn main() -> anyhow::Result<()> {
    let _logging = logging::init()?;
    let result = run();
    if let Err(error) = &result {
        tracing::error!(reason = %logging::error_detail(error), "Application failed");
    }
    tracing::info!(success = result.is_ok(), "Application shutting down");
    result
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--probe" | "--analyze" | "--self-test"))
    {
        return diagnostics::run(&args);
    }
    ui::run()
}
