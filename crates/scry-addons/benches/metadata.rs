mod support;

use criterion::{BenchmarkId, Criterion, Throughput};
use scry_addons::Client;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .configure_from_args();
    let mut group = criterion.benchmark_group("metadata_http");
    for (episodes, concurrent) in [(0, 1), (250, 1), (5_000, 1), (5_000, 8)] {
        let provider = support::Provider::new(episodes)?;
        let client = Client::with_cache_dir(None)?;
        group.throughput(Throughput::Bytes((provider.bytes * concurrent) as u64));
        let mut result = Ok(());
        group.bench_function(
            BenchmarkId::new(format!("fetch_{concurrent}"), episodes),
            |b| {
                b.iter(|| {
                    if result.is_ok() {
                        result = runtime
                            .block_on(support::fetch(&client, &provider, episodes, concurrent));
                    }
                });
            },
        );
        result?;
    }
    group.finish();
    criterion.final_summary();
    Ok(())
}
