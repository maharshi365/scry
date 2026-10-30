mod support;

use criterion::{BenchmarkId, Criterion, Throughput};
use scry_library::{MediaKind, Page};
use std::{hint::black_box, time::Duration};

fn main() -> anyhow::Result<()> {
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .configure_from_args();
    let mut reads = criterion.benchmark_group("library_reads");
    for workload in [
        "up_next", "movies", "series", "episodes", "progress", "home",
    ] {
        // Each case owns its database. Reads do not use an application result cache.
        let db = support::library()?;
        let key = support::series(0).key;
        let page = Page::new(200, 0)?;
        reads.bench_function(workload, |b| {
            b.iter(|| -> anyhow::Result<()> {
                match workload {
                    "up_next" => {
                        black_box(db.list_up_next(black_box(support::NOW), page)?);
                    }
                    "movies" => {
                        black_box(db.list_library(false, Some(MediaKind::Movie), page)?);
                    }
                    "series" => {
                        black_box(db.list_library(false, Some(MediaKind::Series), page)?);
                    }
                    "episodes" => {
                        black_box(db.list_episodes(black_box(&key), page)?);
                    }
                    "progress" => {
                        black_box(db.series_progress(black_box(&key), support::NOW)?);
                    }
                    _ => support::home(&db)?,
                }
                Ok(())
            });
        });
    }
    reads.finish();
    let mut refresh = criterion.benchmark_group("inventory_refresh");
    for count in [250, 5_000] {
        refresh.throughput(Throughput::Elements(u64::from(count)));
        for changed in [false, true] {
            let (mut db, key, mut episodes) = support::inventory(count)?;
            let name = if changed { "one_changed" } else { "unchanged" };
            refresh.bench_with_input(BenchmarkId::new(name, count), &count, |b, _| {
                b.iter(|| -> anyhow::Result<()> {
                    if changed {
                        // Alternate actual stored metadata; every iteration performs a change.
                        episodes[0].released_at =
                            Some(if episodes[0].released_at == Some(support::NOW) {
                                support::NOW - 86_400
                            } else {
                                support::NOW
                            });
                    }
                    black_box(db.sync_episode_releases(black_box(&key), black_box(&episodes))?);
                    Ok(())
                });
            });
        }
    }
    refresh.finish();
    criterion.final_summary();
    Ok(())
}
