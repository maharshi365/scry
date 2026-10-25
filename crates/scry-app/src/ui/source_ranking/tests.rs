use super::*;

fn torrent(name: &str, title: &str, hints: serde_json::Value) -> Stream {
    let mut stream: Stream = serde_json::from_value(serde_json::json!({"name":name,"title":title,"infoHash":"0123456789abcdef0123456789abcdef01234567","fileIdx":0})).unwrap();
    stream.behavior_hints = hints;
    stream
}

#[test]
fn parses_torrentio_seeders_and_binary_display_sizes_without_confusing_pack_numbers() {
    let stream = torrent(
        "Torrentio\n1080p",
        "Season pack 2005 1080p 10bit\nEpisode 2\n👤 1,234 💾 1.5 GB ⚙️ provider",
        serde_json::json!({}),
    );
    assert_eq!(
        Metrics::read(&stream),
        Metrics {
            seeders: Some(1234),
            bytes: Some(1_610_612_736)
        }
    );
    for (text, expected) in [
        ("651.55 MB", 683_199_692),
        ("334.08 MB", 350_308_270),
        ("1.5 GiB", 1_610_612_736),
        ("500 B", 500),
        ("1.5 kB", 1536),
    ] {
        assert_eq!(size(text), Some(expected), "{text}");
    }
}

#[test]
fn structured_file_size_and_seed_hints_take_priority_and_unknown_is_not_zero() {
    let stream = torrent(
        "1080p",
        "👤 2 💾 4 GB",
        serde_json::json!({"seeders":80,"videoSize":123_456_789}),
    );
    assert_eq!(
        Metrics::read(&stream),
        Metrics {
            seeders: Some(80),
            bytes: Some(123_456_789)
        }
    );
    assert_eq!(
        Metrics::read(&torrent("1080p", "No metrics", serde_json::json!({}))),
        Metrics::default()
    );
    assert_eq!(
        Metrics::read(&torrent("1080p", "👤 0", serde_json::json!({}))).seeders,
        Some(0)
    );
}

#[test]
fn malformed_negative_overflow_and_unbounded_metric_lines_are_unknown() {
    for text in [
        "👤 -1 💾 -4 GB",
        "👤 nope 💾 NaN GB",
        "👤 999999999999999999999999999 💾 999999999999999999999 TB",
        "👤 1,2 💾 1,5 GB",
    ] {
        assert_eq!(
            Metrics::read(&torrent("1080p", text, serde_json::json!({}))),
            Metrics::default(),
            "{text}"
        );
    }
    assert_eq!(
        Metrics::read(&torrent(
            "1080p",
            &format!("{} 👤 50 💾 1 GB", "x".repeat(1024)),
            serde_json::json!({})
        )),
        Metrics::default()
    );
}

#[test]
fn weak_and_zero_swarms_do_not_win_just_because_the_file_is_small_or_quality_matches() {
    let policy = SourcePolicy {
        allow_torrents: true,
        quality: "2160".into(),
        ..Default::default()
    };
    let candidates = [
        torrent("Torrentio\n4k", "Weak 👤 1 💾 20 MB", serde_json::json!({})),
        torrent(
            "Torrentio\n1080p",
            "Healthy 👤 120 💾 300 MB",
            serde_json::json!({}),
        ),
        torrent("Torrentio\n4k", "Unknown", serde_json::json!({})),
        torrent("Torrentio\n4k", "Zero 👤 0 💾 1 MB", serde_json::json!({})),
    ]
    .map(|stream| ("Provider".into(), stream))
    .to_vec();
    let selected = select_sources(candidates, &policy);
    assert!(selected.first().unwrap().title.starts_with("Healthy"));
    assert!(selected.last().unwrap().title.starts_with("Zero"));
}

#[test]
fn auto_quality_avoids_tiny_unlabelled_files_and_4k_preference_understands_4k_labels() {
    let candidates = [
        torrent("Torrentio", "Unknown 👤 200 💾 5 MB", serde_json::json!({})),
        torrent(
            "Torrentio\n1080p",
            "HD 👤 180 💾 300 MB",
            serde_json::json!({}),
        ),
        torrent(
            "Torrentio\n4k HDR",
            "UHD 👤 190 💾 3 GB",
            serde_json::json!({}),
        ),
    ]
    .map(|stream| ("Provider".into(), stream))
    .to_vec();
    let policy = SourcePolicy {
        allow_torrents: true,
        ..Default::default()
    };
    assert!(
        select_sources(candidates.clone(), &policy)
            .first()
            .unwrap()
            .title
            .starts_with("HD")
    );
    assert!(
        select_sources(
            candidates,
            &SourcePolicy {
                quality: "2160".into(),
                ..policy
            }
        )
        .first()
        .unwrap()
        .title
        .starts_with("UHD")
    );
}

#[test]
fn missing_file_sizes_do_not_receive_an_unearned_small_file_bonus() {
    let policy = SourcePolicy {
        allow_torrents: true,
        ..Default::default()
    };
    let candidates = [
        torrent("1080p", "Unknown 👤 100", serde_json::json!({})),
        torrent("1080p", "Known 👤 90 💾 300 MB", serde_json::json!({})),
    ]
    .map(|stream| ("Provider".into(), stream))
    .to_vec();
    assert!(
        select_sources(candidates, &policy)
            .first()
            .unwrap()
            .title
            .starts_with("Known")
    );
}

#[test]
fn borrowed_ranking_matches_guide_selection_and_unsupported_swarms_do_not_affect_it() {
    let mut unsupported = torrent(
        "1080p",
        "Unsupported",
        serde_json::json!({"seeders":10000,"videoSize":1}),
    );
    unsupported.info_hash = Some("invalid-hash".into());
    let streams: Vec<(String, Stream)> = vec![
        ("Provider".into(), unsupported),
        (
            "Provider".into(),
            torrent(
                "1080p",
                "Strong",
                serde_json::json!({"seeders":100,"videoSize":300}),
            ),
        ),
        (
            "Provider".into(),
            torrent(
                "1080p",
                "Compact",
                serde_json::json!({"seeders":60,"videoSize":100}),
            ),
        ),
    ];
    let borrowed = rank_sources(
        streams.iter().enumerate(),
        |(_, (provider, stream))| (provider.as_str(), stream),
        "",
    );
    assert_eq!(
        borrowed.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
        [2, 1, 0]
    );
    let guide = select_sources(
        streams.clone(),
        &SourcePolicy {
            allow_torrents: true,
            ..Default::default()
        },
    );
    assert_eq!(
        guide
            .iter()
            .map(|stream| stream.title.as_str())
            .collect::<Vec<_>>(),
        borrowed
            .iter()
            .take(2)
            .map(|(_, (_, stream))| stream.title.as_str())
            .collect::<Vec<_>>()
    );
}

#[derive(serde::Deserialize)]
struct Samples {
    samples: Vec<Sample>,
}
#[derive(serde::Deserialize)]
struct Sample {
    label: String,
    baseline: usize,
    candidates: Vec<(String, u64, String)>,
}

#[test]
fn sampled_torrentio_datasets_choose_well_seeded_compact_hd_instead_of_alphabetical_winners() {
    let samples: Samples = serde_json::from_str(include_str!("torrentio-samples.json")).unwrap();
    let policy = SourcePolicy {
        allow_torrents: true,
        ..Default::default()
    };
    let mut total = 0;
    for sample in samples.samples {
        total += sample.candidates.len();
        let streams: Vec<_> = sample
            .candidates
            .iter()
            .enumerate()
            .map(|(index, (quality, seeders, bytes))| {
                (
                    "Torrentio".into(),
                    torrent(
                        &format!("Torrentio\n{quality}"),
                        &format!("Sample {index}\n👤 {seeders} 💾 {bytes}"),
                        serde_json::json!({}),
                    ),
                )
            })
            .collect();
        let baseline = Metrics::read(&streams.get(sample.baseline).unwrap().1);
        let strongest = sample
            .candidates
            .iter()
            .filter(|(quality, _, _)| matches!(quality.as_str(), "1080p" | "1080p HDR" | "720p"))
            .map(|(_, seeders, _)| *seeders)
            .max()
            .unwrap();
        let selected = select_sources(streams, &policy);
        let winner = selected.first().unwrap();
        let metrics = Metrics::read(winner);
        assert_eq!(quality(winner, ""), 0, "{} did not select HD", sample.label);
        assert!(
            metrics.seeders.unwrap() >= strongest.div_ceil(2),
            "{} selected a weak swarm",
            sample.label
        );
        assert!(
            metrics.seeders >= baseline.seeders || metrics.bytes < baseline.bytes,
            "{} gave up reported peers without reducing file size",
            sample.label
        );
        // Reversed responses must not change a choice based on network completion order.
        let mut reversed: Vec<_> = selected
            .iter()
            .cloned()
            .map(|stream| ("Torrentio".into(), stream))
            .collect();
        reversed.reverse();
        assert_eq!(
            select_sources(reversed, &policy).first().unwrap().title,
            winner.title
        );
    }
    assert_eq!(total, 460);
}
