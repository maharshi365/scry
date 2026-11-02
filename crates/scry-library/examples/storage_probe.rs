//! Measure an isolated database copy without printing media identities.
use std::{hint::black_box, path::PathBuf, time::Instant};

use anyhow::{Context, Result, ensure};
use scry_library::{Library, MediaKind, Page, StatsPeriod};

#[expect(
    clippy::print_stdout,
    reason = "This diagnostic prints aggregate timings only"
)]
fn main() -> Result<()> {
    let path = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("provide an isolated database copy")?,
    );
    let path = path.canonicalize()?;
    let sandbox = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/storage-redesign")
        .canonicalize()?;
    ensure!(
        path.starts_with(sandbox),
        "benchmark database must be an isolated copy under target/storage-redesign"
    );
    let start = Instant::now();
    let mut library = Library::open(path)?;
    println!("open_ms={:.3}", start.elapsed().as_secs_f64() * 1000.0);
    if std::env::args_os()
        .nth(2)
        .is_some_and(|argument| argument == "--upgrade-only")
    {
        return Ok(());
    }
    let items = library.list_watched(Page::new(200, 0)?)?;
    for kind in [MediaKind::Movie, MediaKind::Episode] {
        let item = items
            .iter()
            .find(|item| item.key.kind == kind)
            .context("missing watched fixture")?;
        let mut writes = Vec::new();
        let mut evaluations = Vec::new();
        let mut reads = Vec::new();
        for index in 0..30 {
            let start = Instant::now();
            library.set_watched(&item.key, index % 2 == 1)?;
            writes.push(start.elapsed().as_secs_f64() * 1000.0);
            let start = Instant::now();
            black_box(library.evaluate_achievements()?);
            evaluations.push(start.elapsed().as_secs_f64() * 1000.0);
            let start = Instant::now();
            black_box(library.detail_snapshot(Some(&item.key), &[])?);
            reads.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        for (name, mut values) in [
            ("write", writes),
            ("achievements", evaluations),
            ("snapshot", reads),
        ] {
            values.sort_by(f64::total_cmp);
            println!(
                "{kind:?}_{name}_median_ms={:.3}",
                values.get(15).context("missing timing")?
            );
        }
    }
    let start = Instant::now();
    black_box(library.nerd_stats(StatsPeriod::AllTime)?);
    println!("dashboard_ms={:.3}", start.elapsed().as_secs_f64() * 1000.0);
    Ok(())
}
