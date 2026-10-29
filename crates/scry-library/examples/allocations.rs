#[path = "../benches/support/mod.rs"]
mod support;

use std::io::{self, Write};

fn main() -> anyhow::Result<()> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    writeln!(
        output,
        "workload,allocations,rust_bytes_total,rust_bytes_peak"
    )?;
    let db = support::library()?;
    let mut result = Ok(());
    let info = allocation_counter::measure(|| result = support::home(&db));
    result?;
    writeln!(
        output,
        "home,{},{},{}",
        info.count_total, info.bytes_total, info.bytes_max
    )?;
    for count in [250, 5_000] {
        let (mut db, key, episodes) = support::inventory(count)?;
        let mut result = Ok(false);
        let info =
            allocation_counter::measure(|| result = db.sync_episode_releases(&key, &episodes));
        result?;
        writeln!(
            output,
            "refresh_{count},{},{},{}",
            info.count_total, info.bytes_total, info.bytes_max
        )?;
    }
    Ok(())
}
