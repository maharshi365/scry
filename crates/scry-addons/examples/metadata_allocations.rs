#[path = "../benches/support/mod.rs"]
mod support;

use scry_addons::Client;
use std::io::{self, Write};

fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut output = io::BufWriter::new(io::stdout().lock());
    writeln!(
        output,
        "episodes,concurrent,wire_bytes,allocations,rust_bytes_total,rust_bytes_peak,rust_bytes_retained"
    )?;
    for (episodes, concurrent) in [(0, 1), (250, 1), (5_000, 1), (5_000, 8)] {
        let provider = support::Provider::new(episodes)?;
        let client = Client::with_cache_dir(None)?;
        runtime.block_on(support::fetch(&client, &provider, episodes, concurrent))?;
        let mut result: anyhow::Result<()> = Ok(());
        let info = allocation_counter::measure(|| {
            result = runtime.block_on(async {
                // Ten uncached batches expose retention across repeated refreshes.
                for _ in 0..10 {
                    support::fetch(&client, &provider, episodes, concurrent).await?;
                }
                Ok(())
            });
        });
        result?;
        writeln!(
            output,
            "{episodes},{concurrent},{},{},{},{},{}",
            provider.bytes * concurrent,
            info.count_total,
            info.bytes_total,
            info.bytes_max,
            info.bytes_current
        )?;
    }
    Ok(())
}
