use super::{
    filename::{parse, parse_in_roots},
    *,
};
use scry_addons::Meta;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn parses_movies_and_numbered_episodes() {
    let movie = parse(Path::new("Arrival.2016.1080p.BluRay.mkv"));
    assert_eq!(
        (movie.title.as_str(), movie.year, movie.episode),
        ("Arrival", Some(2016), None)
    );
    for name in ["The.Office.S01E02E03.mkv", "The.Office.1x02-03.mkv"] {
        let show = parse(Path::new(name));
        assert_eq!(show.title, "The Office");
        assert_eq!(
            show.episode,
            Some(Episode {
                season: 1,
                numbers: vec![2, 3]
            })
        );
    }
    let show = parse(Path::new("Shows/Example/Season 0/S00E01.mkv"));
    assert_eq!(show.title, "Example");
    assert_eq!(show.episode.unwrap().season, 0);
}

#[test]
fn numeric_titles_are_not_mistaken_for_release_years() {
    let movie = parse(Path::new("2001.A.Space.Odyssey.1968.1080p.mkv"));
    assert_eq!(movie.title, "2001 A Space Odyssey");
    assert_eq!(movie.year, Some(1968));
    let episode = parse(Path::new("Example.S01E01.1984.mkv"));
    assert_eq!(
        episode.year, None,
        "Episode titles must not become show years"
    );
}

#[test]
fn movie_release_patterns_keep_title_words_and_drop_technical_suffixes() {
    let cases = [
        (
            "The.Matrix.(1999).1080p.BluRay.x264-GROUP.mkv",
            "The Matrix",
            Some(1999),
        ),
        (
            "[YTS.MX] The_Matrix_1999_2160p_WEB-DL.mkv",
            "The Matrix",
            Some(1999),
        ),
        (
            "Arrival {release-group} (2016) HEVC 10bit.mkv",
            "Arrival",
            Some(2016),
        ),
        ("Arrival.BDRip.x264-GROUP.avi", "Arrival", None),
        ("Arrival.PROPER.720p.WEBRip.mkv", "Arrival", None),
        ("Arrival.REMUX.UHD.TrueHD.Atmos.mkv", "Arrival", None),
        ("Arrival.4k.HDR.HEVC.mkv", "Arrival", None),
        ("Arrival.H.264.AAC.mkv", "Arrival", None),
        ("Arrival.EXTENDED.1080p.mkv", "Arrival", None),
        ("Alien.Directors.Cut.1080p.mkv", "Alien", None),
        ("1917.2019.1080p.mkv", "1917", Some(2019)),
        ("1917.mkv", "1917", None),
        ("1984.1080p.mkv", "1984", None),
        (
            "Blade.Runner.2049.(2017).mkv",
            "Blade Runner 2049",
            Some(2017),
        ),
        ("[REC].(2007).1080p.mkv", "REC", Some(2007)),
        ("Rocky.IV.1985.mkv", "Rocky IV", Some(1985)),
        ("I.Am.Legend.2007.mkv", "I Am Legend", Some(2007)),
        ("The.Web.1947.mkv", "The Web", Some(1947)),
        (
            "Spider-Man.No.Way.Home.2021.mkv",
            "Spider-Man No Way Home",
            Some(2021),
        ),
        ("Amélie.(2001).mkv", "Amélie", Some(2001)),
    ];
    for (filename, title, year) in cases {
        let parsed = parse(Path::new(filename));
        assert_eq!(parsed.title, title, "{filename}");
        assert_eq!(parsed.year, year, "{filename}");
    }
}

#[test]
fn leading_codec_and_language_tags_do_not_hide_volume_numbers() {
    for name in [
        "DIvX-ITa Kill Bill Vol. 2",
        "(Divx-Ita )Kill Bill Vol. 2",
        "Divx- ita Kill Bill Vol. 2",
        "(Divx - ita) - Kill Bill Vol. 2",
        "xvid- ita Kill Bill Vol. 2",
        "(xvid-ita) - Kill Bill Vol. 2",
        "xvidita Kill Bill Vol. 2",
        "(dvdrip) Kill Bill Vol. 2",
        "[Divx-Ita ]Kill Bill Vol. 2",
        "Kill Bill Vol. 2 [divx - Ita ]",
    ] {
        let filename = format!("{name}.avi");
        let parsed = parse(Path::new(&filename));
        assert_eq!(parsed.title, "Kill Bill Vol 2", "{filename}");
        assert_eq!(parsed.year, None, "{filename}");
    }
}

#[test]
fn scanner_episode_patterns_do_not_guess_ambiguous_absolute_numbers() {
    for filename in [
        "Example.s1e2.mkv",
        "Example.01x02.mkv",
        "Example.S01.E02.mkv",
        "Example.1st.Season.02.mkv",
        "Example Season 01 Episode 02.mkv",
    ] {
        let parsed = parse(Path::new(filename));
        assert_eq!(parsed.title, "Example", "{filename}");
        assert_eq!(
            parsed.episode,
            Some(Episode {
                season: 1,
                numbers: vec![2]
            }),
            "{filename}"
        );
    }
    let parsed = parse(Path::new("Example.2nd.Season.03.mkv"));
    assert_eq!(
        parsed.episode,
        Some(Episode {
            season: 2,
            numbers: vec![3]
        })
    );
    assert_eq!(parsed.title, "Example");
    assert_eq!(parse(Path::new("Naruto - 012.mkv")).episode, None);
    assert_eq!(
        parse(Path::new("The.French.Connection.1971.mkv")).title,
        "The French Connection"
    );
}

#[test]
fn explicit_ranges_cover_multiple_episodes_without_inventing_a_cross_season_mapping() {
    for (filename, numbers) in [
        ("Example.S01E01-E05.mkv", vec![1, 2, 3, 4, 5]),
        ("Example.1x01-1x02.mkv", vec![1, 2]),
    ] {
        assert_eq!(
            parse(Path::new(filename)).episode,
            Some(Episode { season: 1, numbers }),
            "{filename}"
        );
    }
    assert!(parse(Path::new("Example.1x01-2x02.mkv")).episode.is_none());
    assert!(parse(Path::new("Example.01-50.mkv")).episode.is_none());
}

#[test]
fn folder_context_stops_at_the_selected_root() {
    let path = Path::new("Outside/Example/Season 2/03.mkv");
    let parsed = parse_in_roots(path, &[PathBuf::from("Outside/Example")]);
    assert_eq!(parsed.title, "Example");
    assert_eq!(
        parsed.episode,
        Some(Episode {
            season: 2,
            numbers: vec![3]
        })
    );
    let bounded = parse_in_roots(path, &[PathBuf::from("Outside/Example/Season 2")]);
    assert_eq!(
        bounded.title, "03",
        "Do not read the unselected parent show folder"
    );
}

#[test]
fn scanner_accepts_video_extensions_case_insensitively_not_images_or_audio() {
    let root = tempfile::tempdir().unwrap();
    for name in [
        "Movie.2020.MkV",
        "Other.2021.WMV",
        "Film.2019.FLV",
        "ignore.GIF",
        "ignore.mp3",
        "ignore.torrent",
    ] {
        fs::write(root.path().join(name), []).unwrap();
    }
    let mut index = Index::default();
    index.add_root(root.path()).unwrap();
    index.scan();
    assert_eq!(index.items.len(), 3);
    assert!(index.items.iter().all(|item| item.title != "ignore"));
}

#[test]
fn folders_supply_missing_movie_titles_and_explicit_season_numbers() {
    let movie = parse(Path::new("Arrival (2016)/video.mkv"));
    assert_eq!((movie.title.as_str(), movie.year), ("Arrival", Some(2016)));
    let show = parse(Path::new("Example/Season 2/03 - A title.mkv"));
    assert_eq!(show.title, "Example");
    assert_eq!(
        show.episode,
        Some(Episode {
            season: 2,
            numbers: vec![3]
        })
    );
}

#[test]
fn an_unnumbered_file_stays_playable_after_matching_to_a_series() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("unknown.mkv"), []).unwrap();
    let mut index = Index::default();
    index.add_root(root.path()).unwrap();
    index.scan();
    let meta: Meta = serde_json::from_value(
        serde_json::json!({"id":"ttshow", "type":"series", "name":"Example"}),
    )
    .unwrap();
    index.items[0].matched = Some(Match::from_meta(&meta, true));
    let detail = index.items[0].meta().unwrap();
    assert_eq!(detail.videos.len(), 1);
    assert_eq!(
        index.items[0].sources("series", &detail.videos[0].id).len(),
        1
    );
}

#[test]
fn scan_groups_deduplicates_and_preserves_corrections() {
    let root = tempfile::tempdir().unwrap();
    let shows = root.path().join("Shows");
    fs::create_dir(&shows).unwrap();
    let path = shows.join("Example.S01E01.mkv");
    fs::write(&path, []).unwrap();
    fs::write(shows.join("Example.S01E02.MKV"), []).unwrap();
    fs::write(shows.join("ignore.txt"), []).unwrap();
    let mut index = Index::default();
    index.add_root(root.path()).unwrap();
    index.add_root(&shows).unwrap();
    assert_eq!(index.scan(), 0);
    assert_eq!(index.items.len(), 1);
    assert_eq!(index.items[0].files.len(), 2);
    let id = index.items[0].id.clone();
    let meta = index.items[0].meta().unwrap();
    index.items[0].matched = Some(Match::from_meta(&meta, true));
    index.items[0].files[0].episode = Some(Episode {
        season: 2,
        numbers: vec![5],
    });
    index.items[0].files[0].corrected = true;
    index.scan();
    assert_eq!(index.items[0].id, id);
    assert!(index.items[0].matched.as_ref().unwrap().manual);
    assert_eq!(index.items[0].files[0].episode.as_ref().unwrap().season, 2);
    fs::remove_file(path).unwrap();
    index.scan();
    assert_eq!(index.items[0].files.len(), 1);
}

#[test]
fn offline_roots_retain_unavailable_files_and_matches_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("media");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("Arrival.2016.mkv"), []).unwrap();
    let mut index = Index::default();
    index.add_root(&root).unwrap();
    index.scan();
    let meta: Meta = serde_json::from_value(
        serde_json::json!({"id":"tt2543164", "type":"movie", "name":"Arrival"}),
    )
    .unwrap();
    index.items[0].matched = Some(Match::from_meta(&meta, true));
    save(dir.path(), &index).unwrap();
    save(dir.path(), &index).unwrap();
    let mut index = load(dir.path()).unwrap();
    assert_eq!(index.items[0].sources("movie", "tt2543164").len(), 1);
    fs::rename(&root, dir.path().join("disconnected")).unwrap();
    assert!(index.scan() > 0);
    assert_eq!(index.items.len(), 1);
    assert!(index.items[0].sources("movie", "tt2543164").is_empty());
    assert!(index.items[0].matched.as_ref().unwrap().manual);
}

#[test]
fn anime_files_group_without_guessing_season_one_and_new_files_use_the_saved_order() {
    let root = tempfile::tempdir().unwrap();
    for name in ["[Group] Example - 001v2 [1080p].mkv", "Example - 002.mkv"] {
        fs::write(root.path().join(name), []).unwrap();
    }
    let mut index = Index::default();
    index.add_root(root.path()).unwrap();
    index.scan();
    assert_eq!(index.items.len(), 1);
    assert_eq!(index.items[0].title, "Example");
    assert_eq!(index.items[0].kind, "series");
    assert!(
        index.items[0]
            .files
            .iter()
            .all(|file| file.episode.is_none())
    );
    let meta: Meta = serde_json::from_value(
        serde_json::json!({"id":"kitsu:42", "type":"series", "name":"Example", "videos":[
            {"id":"kitsu:42:1", "season":1, "episode":1, "title":"First"},
            {"id":"kitsu:42:2", "season":1, "episode":2, "title":"Second"},
            {"id":"kitsu:42:3", "season":1, "episode":3, "title":"Third"}
        ]}),
    )
    .unwrap();
    index.items[0].matched = Some(Match::from_meta(&meta, true));
    index.items[0].ordering = Some(Ordering::from_meta(&meta, EpisodeOrder::Absolute).unwrap());
    let data = tempfile::tempdir().unwrap();
    save(data.path(), &index).unwrap();
    let mut index = load(data.path()).unwrap();
    fs::write(root.path().join("Example - 003.mkv"), []).unwrap();
    index.scan();
    assert_eq!(index.items[0].sources("series", "kitsu:42:3").len(), 1);
    assert_eq!(index.items[0].meta().unwrap().videos[2].id, "kitsu:42:3");
}

#[test]
fn dvd_display_numbering_does_not_change_the_canonical_stream_identity() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Example.S01E01.mkv"), []).unwrap();
    let mut index = Index::default();
    index.add_root(root.path()).unwrap();
    index.scan();
    let meta: Meta =
        serde_json::from_value(serde_json::json!({"id":"show", "type":"series", "name":"Example"}))
            .unwrap();
    index.items[0].matched = Some(Match::from_meta(&meta, true));
    index.items[0].ordering = Some(Ordering {
        series_id: "show".into(),
        mode: EpisodeOrder::Dvd,
        name: "DVD".into(),
        source: None,
        episodes: vec![OrderedEpisode {
            file_season: 1,
            file_episode: 1,
            id: "show:1:11".into(),
            title: "Pilot".into(),
            season: 1,
            episode: 11,
            released: None,
        }],
    });
    let detail = index.items[0].meta().unwrap();
    assert_eq!(detail.videos[0].episode, Some(1));
    assert_eq!(detail.videos[0].id, "show:1:11");
    assert_eq!(index.items[0].sources("series", "show:1:11").len(), 1);
    assert!(index.items[0].sources("series", "show:1:1").is_empty());
}

#[test]
fn an_unverified_anime_match_does_not_invent_provider_episode_ids() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("Example.S01E01.mkv"), []).unwrap();
    let mut index = Index::default();
    index.add_root(root.path()).unwrap();
    index.scan();
    let meta: Meta = serde_json::from_value(
        serde_json::json!({"id":"kitsu:42", "type":"series", "name":"Example"}),
    )
    .unwrap();
    index.items[0].matched = Some(Match::from_meta(&meta, true));
    let detail = index.items[0].meta().unwrap();
    assert!(detail.videos[0].id.starts_with("local-file:"));
    assert_eq!(
        index.items[0].sources("series", &detail.videos[0].id).len(),
        1
    );
    assert!(index.items[0].sources("series", "kitsu:42:1:1").is_empty());
}
