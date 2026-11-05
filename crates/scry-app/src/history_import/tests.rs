use super::*;
use serde_json::json;

fn item(kind: &str, id: &str, state: Value) -> Result<Item> {
    let mut value = json!({"_id":id,"type":kind,"name":"Sample title","removed":true,"temp":true});
    value["state"] = state;
    Ok(serde_json::from_value(value)?)
}

fn inventory() -> Result<Meta> {
    Ok(serde_json::from_value(
        json!({"id":"tt100","type":"series","name":"Sample show","videos":[
            {"id":"tt100:1:3","season":1,"episode":3,"released":"2020-01-03"},
            {"id":"tt100:1:1","season":1,"episode":1,"released":"2020-01-01"},
            {"id":"tt100:1:2","season":1,"episode":2,"released":"2020-01-02"}
        ]}),
    )?)
}

fn bitmap(anchor: &str, length: usize, bytes: &[u8]) -> Result<String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes)?;
    Ok(format!(
        "{anchor}:{length}:{}",
        STANDARD.encode(encoder.finish()?)
    ))
}

#[test]
fn parser_accepts_stremio_buckets_and_skips_bad_records_without_exposing_auth() -> Result<()> {
    let good = json!({"_id":"tt100","type":"movie","name":"Sample","state":{"timesWatched":1}});
    for value in [
        json!([good]),
        json!({"items":{"tt100":good}}),
        json!({"library":[good]}),
        json!({"libraryItems":[good]}),
    ] {
        assert_eq!(parse(&serde_json::to_vec(&value)?)?.0.len(), 1);
    }
    let value = json!({"library":{"items":{"tt100":good}},"library_recent":{"items":{"bad":{"name":"https://private.invalid/token","state":{}}}},"profile":{"auth":{"key":"DO_NOT_IMPORT"}}});
    let (items, preview) = parse(&serde_json::to_vec(&value)?)?;
    assert_eq!(items.len(), 1);
    assert_eq!(preview.issues.len(), 1);
    assert_eq!(preview.issues[0].title, "Untitled record");
    assert!(!format!("{items:?} {preview:?}").contains("DO_NOT_IMPORT"));
    Ok(())
}

#[test]
fn newer_duplicate_state_wins_and_is_reported() -> Result<()> {
    let make = |date, times| json!({"_id":"tt100","type":"movie","name":"Sample","_mtime":date,"state":{"timesWatched":times}});
    for rows in [
        vec![make("2020-01-01", 1), make("2021-01-01", 0)],
        vec![make("2021-01-01", 0), make("2020-01-01", 1)],
    ] {
        let (items, preview) = parse(&serde_json::to_vec(&rows)?)?;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].state.times_watched, 0);
        assert_eq!(preview.issues.len(), 1);
    }
    Ok(())
}

#[test]
fn conflicting_accounts_or_undated_duplicates_are_not_merged() -> Result<()> {
    let make = |times| json!({"_id":"tt100","type":"movie","name":"Sample","state":{"timesWatched":times}});
    let (items, preview) = parse(&serde_json::to_vec(&json!([make(0), make(1)]))?)?;
    assert!(items.is_empty());
    assert!(preview.issues[0].reason.contains("Conflicting duplicate"));
    let mixed = json!({"library":{"uid":"one","items":{"tt100":make(1)}},"library_recent":{"uid":"two","items":{}}});
    assert!(parse(&serde_json::to_vec(&mixed)?).is_err());
    Ok(())
}

#[test]
fn duplicate_or_mismatched_metadata_does_not_transfer_watched_flags() -> Result<()> {
    let mut meta = inventory()?;
    let duplicate = meta.videos[0].clone();
    meta.videos.push(duplicate);
    let mut preview = Preview::default();
    append(
        &mut preview,
        &item(
            "series",
            "tt100",
            json!({"watched":bitmap("tt100:1:3",3,&[7])?}),
        )?,
        Some(&meta),
    );
    assert!(preview.input.entries.is_empty());
    assert!(
        preview
            .issues
            .iter()
            .any(|i| i.reason.contains("duplicate"))
    );
    let mut meta = inventory()?;
    meta.id = "tt999".into();
    let mut preview = Preview::default();
    append(
        &mut preview,
        &item(
            "series",
            "tt100",
            json!({"watched":bitmap("tt100:1:3",3,&[7])?}),
        )?,
        Some(&meta),
    );
    assert!(preview.input.entries.is_empty());
    assert!(preview.issues.iter().any(|i| i.reason.contains("anchor")));
    Ok(())
}

#[test]
fn movie_completion_requires_explicit_evidence_not_position_or_elapsed_time() -> Result<()> {
    let mut preview = Preview::default();
    append(
        &mut preview,
        &item(
            "movie",
            "tt100",
            json!({"timeOffset":95_000,"duration":100_000,"timeWatched":99_000}),
        )?,
        None,
    );
    assert_eq!(preview.input.entries.len(), 1);
    assert!(!preview.input.entries[0].watched);
    assert_eq!(
        preview.input.entries[0].progress,
        Progress::new(95.0, 100.0)
    );
    for field in ["timesWatched", "flaggedWatched"] {
        let mut preview = Preview::default();
        append(
            &mut preview,
            &item("movie", "tt100", json!({field:1}))?,
            None,
        );
        assert!(preview.input.entries[0].watched);
        assert!(preview.input.entries[0].progress.is_none());
    }
    Ok(())
}

#[test]
fn series_bitmap_uses_numbered_episode_order_and_preserves_resume() -> Result<()> {
    let mut preview = Preview::default();
    let watched = bitmap("tt100:1:3", 3, &[0b101])?;
    append(
        &mut preview,
        &item(
            "series",
            "tt100",
            json!({"watched":watched,"video_id":"tt100:1:2","timeOffset":603_019,"duration":2_497_920}),
        )?,
        Some(&inventory()?),
    );
    assert!(preview.issues.is_empty(), "{:?}", preview.issues);
    assert_eq!(preview.input.entries.len(), 3);
    let ids: Vec<_> = preview
        .input
        .entries
        .iter()
        .filter(|e| e.watched)
        .map(|e| e.media.key.id.as_str())
        .collect();
    assert_eq!(ids, ["tt100:1:1", "tt100:1:3"]);
    let resume = &preview.input.entries[1];
    assert_eq!(resume.progress, Progress::new(603.019, 2497.92));
    assert!(resume.active);
    assert_eq!(preview.input.parents.len(), 1);
    assert_eq!(preview.input.inventory.len(), 3);
    Ok(())
}

#[test]
fn bitmap_mapping_failure_keeps_exact_resume_and_reports_partial_import() -> Result<()> {
    for watched in [
        bitmap("tt100:1:9", 3, &[7])?,
        bitmap("tt100:1:3", 2, &[3])?,
        "bad payload".into(),
        bitmap("tt100:1:2", 2, &[0b110])?,
    ] {
        let mut preview = Preview::default();
        append(
            &mut preview,
            &item(
                "series",
                "tt100",
                json!({"watched":watched,"video_id":"tt100:1:2","timeOffset":5000,"duration":10000}),
            )?,
            Some(&inventory()?),
        );
        assert_eq!(preview.input.entries.len(), 1);
        assert!(!preview.input.entries[0].watched);
        assert!(preview.input.entries[0].progress.is_some());
        assert!(
            preview
                .issues
                .iter()
                .any(|i| i.reason.starts_with("Episode history skipped"))
        );
    }
    Ok(())
}

#[test]
fn unknown_or_future_episodes_do_not_become_watched() -> Result<()> {
    let mut meta = inventory()?;
    for video in &mut meta.videos {
        video.released = Some("2100-01-01".into());
    }
    let mut preview = Preview::default();
    append(
        &mut preview,
        &item(
            "series",
            "tt100",
            json!({"watched":bitmap("tt100:1:3",3,&[7])?}),
        )?,
        Some(&meta),
    );
    assert!(preview.input.entries.is_empty());
    assert!(
        preview
            .issues
            .iter()
            .any(|i| i.reason.contains("release date"))
    );
    Ok(())
}

#[test]
fn unsupported_ids_and_video_mismatches_are_not_guessed() -> Result<()> {
    for (kind, id) in [
        ("series", "kitsu:47759"),
        ("series", "tmdb:295778"),
        ("tv", "ustv-channel"),
        ("movie", "tt123:1:1"),
    ] {
        let mut preview = Preview::default();
        append(
            &mut preview,
            &item(kind, id, json!({"timesWatched":1}))?,
            None,
        );
        assert!(preview.input.entries.is_empty());
        assert!(preview.issues[0].reason.contains("Unsupported"));
    }
    for video in [
        "tt999:1:2",
        "tt100:01:2",
        "tt100:1:0",
        "tt100:+1:2",
        "tt100:1:+2",
        "tt100:1:02",
        "tt100:4294967296:2",
        "tt100:1:2:3",
        "",
    ] {
        let mut preview = Preview::default();
        append(
            &mut preview,
            &item(
                "series",
                "tt100",
                json!({"video_id":video,"timeOffset":10,"duration":100}),
            )?,
            None,
        );
        assert!(preview.input.entries.is_empty());
        assert!(preview.issues.iter().any(|i| i.reason.contains("video ID")));
    }
    Ok(())
}

#[test]
fn resume_updates_watched_episode_without_replacing_its_metadata() -> Result<()> {
    let mut preview = Preview::default();
    append(
        &mut preview,
        &item(
            "series",
            "tt100",
            json!({"watched":bitmap("tt100:1:3",3,&[7])?,"video_id":"tt100:1:2","timeOffset":5000,"duration":10000}),
        )?,
        Some(&inventory()?),
    );
    assert!(preview.issues.is_empty(), "{:?}", preview.issues);
    assert_eq!(preview.input.entries.len(), 3);
    let resume = &preview.input.entries[1];
    assert_eq!(resume.media.key.id, "tt100:1:2");
    assert_eq!(resume.media.title, "Sample show · S1 E2 · tt100:1:2");
    assert!(resume.watched);
    assert!(resume.active);
    assert_eq!(resume.progress, Progress::new(5.0, 10.0));
    Ok(())
}

#[test]
fn bitmap_rejects_repeated_episode_positions_with_different_release_dates() -> Result<()> {
    let mut releases = crate::media_metadata::episode_inventory(
        &MediaKey::new("imdb", MediaKind::Series, "tt100"),
        &inventory()?,
    );
    let mut repeated = releases[1].clone();
    repeated.released_at = Some(1);
    releases.push(repeated);
    assert!(bitfield::decode(&bitmap("tt100:1:3", 3, &[7])?, "tt100", &releases).is_err());
    Ok(())
}

#[test]
fn removed_resume_is_retained_but_not_reactivated() -> Result<()> {
    let mut source = item("movie", "tt100", json!({"timeOffset":10,"duration":100}))?;
    source.temp = false;
    let mut preview = Preview::default();
    append(&mut preview, &source, None);
    assert!(!preview.input.entries[0].active);
    assert!(preview.input.entries[0].progress.is_some());
    Ok(())
}

#[test]
fn invalid_progress_does_not_drop_valid_movie_watched_state() -> Result<()> {
    for state in [
        json!({"timeOffset":100,"duration":0,"timesWatched":1}),
        json!({"timeOffset":101,"duration":100,"timesWatched":1}),
        json!({"timeOffset":u64::MAX,"duration":u64::MAX,"timesWatched":1}),
    ] {
        let mut preview = Preview::default();
        append(&mut preview, &item("movie", "tt100", state)?, None);
        assert!(preview.input.entries[0].watched);
        assert!(preview.input.entries[0].progress.is_none());
        assert!(
            preview
                .issues
                .iter()
                .any(|i| i.reason.contains("invalid duration"))
        );
    }
    Ok(())
}

#[test]
fn malformed_containers_and_excessive_records_are_rejected() -> Result<()> {
    for input in [
        b"invalid".as_slice(),
        b"{}",
        b"{\"profile\":{\"auth\":{}}}",
        b"{\"library\":123}",
    ] {
        assert!(parse(input).is_err());
    }
    assert!(parse(&serde_json::to_vec(&vec![json!({}); MAX_ITEMS + 1])?).is_err());
    let bomb = bitmap("tt100:1:3", 3, &vec![0; 100_000])?;
    let meta = inventory()?;
    assert!(
        bitfield::decode(
            &bomb,
            "tt100",
            &crate::media_metadata::episode_inventory(
                &MediaKey::new("imdb", MediaKind::Series, "tt100"),
                &meta
            )
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn truncated_bitmap_and_trailing_compressed_data_are_reported_not_imported() -> Result<()> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let encoded = bitmap("tt100:1:3", 3, &[7])?;
    let (_, payload) = encoded
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("fixture payload"))?;
    let bytes = STANDARD.decode(payload)?;
    let inventory = crate::media_metadata::episode_inventory(
        &MediaKey::new("imdb", MediaKind::Series, "tt100"),
        &inventory()?,
    );
    for bytes in [
        bytes[..bytes.len() - 1].to_vec(),
        [bytes.as_slice(), b"trailing"].concat(),
    ] {
        let encoded = format!("tt100:1:3:3:{}", STANDARD.encode(bytes));
        assert!(bitfield::decode(&encoded, "tt100", &inventory).is_err());
    }
    Ok(())
}

#[test]
fn file_import_preserves_source_bytes_and_imports_only_verified_history() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("stremio.json");
    let bytes = serde_json::to_vec(&json!({"library":[
        {"_id":"tt100","type":"movie","name":"Sample movie","state":{"timesWatched":1}},
        {"_id":"tt101","type":"movie","name":"Unfinished movie","removed":true,"temp":true,"state":{"timeOffset":10_000,"duration":100_000}},
        {"_id":"kitsu:100","type":"series","name":"Unmapped anime","state":{"timesWatched":1}}
    ]}))?;
    std::fs::write(&path, &bytes)?;
    let (items, mut preview) = read(&path)?;
    for item in items {
        append(&mut preview, &item, None);
    }
    assert_eq!(preview.titles, 2);
    assert_eq!(preview.issues.len(), 1);
    let mut library = scry_library::Library::memory()?;
    let result = library.import_history(&preview.input)?;
    assert_eq!(result.watched, 1);
    assert_eq!(result.progress, 1);
    assert_eq!(
        library.list_currently_watching(scry_library::Page::default())?[0]
            .0
            .key
            .id,
        "tt101"
    );
    assert_eq!(
        std::fs::read(&path)?,
        bytes,
        "Import must not alter the source export"
    );
    let missing = directory.path().join("private-credential-token.json");
    let error = read(&missing)
        .err()
        .ok_or_else(|| anyhow::anyhow!("missing file must fail"))?;
    assert!(!error.to_string().contains("private-credential-token"));
    Ok(())
}
