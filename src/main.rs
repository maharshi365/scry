mod diagnostics;
mod library_worker;
mod services;
mod search;
mod artwork;
mod artwork_cache;
mod ui;
use scry::{playback, telemetry};

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
