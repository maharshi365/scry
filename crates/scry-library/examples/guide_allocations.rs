use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    hint::black_box,
    io::{self, Write},
    time::{Duration, Instant},
};

use scry_library::{
    EpisodeRelease, Library, Media, MediaKey, MediaKind,
    guide::{EpisodeOrder, GuideAction, GuideMetadata, Snapshot, SourcePolicy},
};

const NOW: i64 = 1_700_000_000;

fn fixture() -> anyhow::Result<Library> {
    let mut library = Library::memory()?;
    for show in 0..12 {
        let parent = Media {
            key: MediaKey::new("benchmark", MediaKind::Series, format!("show-{show}")),
            title: format!("Show {show}"),
            parent: None,
        };
        library.upsert_media(&parent)?;
        library.add_to_watchlist(&parent.key)?;
        library.set_guide_metadata(
            &parent,
            &GuideMetadata {
                genres: vec!["Comedy".into(), "Drama".into()],
                description: "Program description. ".repeat(16),
                ..GuideMetadata::default()
            },
        )?;
        let episodes: Vec<_> = (0..100)
            .map(|episode| EpisodeRelease {
                media: Media {
                    key: MediaKey::new(
                        "benchmark",
                        MediaKind::Episode,
                        format!("show-{show}-episode-{episode}"),
                    ),
                    title: format!("Episode {episode}"),
                    parent: Some(parent.key.clone()),
                },
                released_at: Some(NOW - 86_400),
                season: Some(episode / 20 + 1),
                episode: Some(episode % 20 + 1),
            })
            .collect();
        library.sync_episode_releases(&parent.key, &episodes)?;
        if let Some(episode) = episodes.first() {
            library.set_watched(&episode.media.key, true)?;
        }
    }
    Ok(library)
}

fn measure(
    library: &mut Library,
    action: GuideAction,
    workload: &str,
    sample: u32,
    output: &mut impl Write,
) -> anyhow::Result<()> {
    let mut result = Ok(Snapshot::default());
    let mut elapsed = Duration::ZERO;
    let info = allocation_counter::measure(|| {
        let started = Instant::now();
        result = library.guide_update(action, NOW);
        elapsed = started.elapsed();
    });
    let snapshot = result?;
    let slots: usize = snapshot
        .channels
        .iter()
        .map(|channel| channel.slots.len())
        .sum();
    anyhow::ensure!(slots > 0, "guide fixture produced no slots");
    let mut fingerprint = DefaultHasher::new();
    serde_json::to_vec(&snapshot.channels)?.hash(&mut fingerprint);
    if sample > 0 {
        writeln!(
            output,
            "{workload},{sample},{},{},{},{},{},{},{:016x}",
            elapsed.as_nanos(),
            info.count_total,
            info.bytes_total,
            info.bytes_max,
            snapshot.channels.len(),
            slots,
            fingerprint.finish(),
        )?;
    }
    black_box(snapshot);
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let mut output = io::BufWriter::new(io::stdout().lock());
    writeln!(
        output,
        "workload,sample,elapsed_ns,allocations,rust_bytes_total,rust_bytes_peak,channels,slots,schedule_hash"
    )?;
    for sample in 0..6 {
        let mut library = fixture()?;
        for (workload, action) in [
            ("initial", GuideAction::Refresh),
            ("unchanged", GuideAction::Refresh),
            (
                "sequential_refill",
                GuideAction::Policy(SourcePolicy {
                    order: EpisodeOrder::Sequential,
                    ..SourcePolicy::default()
                }),
            ),
            (
                "shuffle_refill",
                GuideAction::Policy(SourcePolicy::default()),
            ),
        ] {
            measure(&mut library, action, workload, sample, &mut output)?;
        }
    }
    Ok(())
}
