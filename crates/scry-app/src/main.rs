#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod artwork;
mod artwork_cache;
mod diagnostics;
mod library_worker;
mod local_media;
mod media_metadata;
mod search;
mod services;
mod ui;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--probe" | "--analyze" | "--self-test"))
    {
        return diagnostics::run(&args);
    }
    ui::run()
}
