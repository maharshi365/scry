use super::local_media::{Hints, infer, select_match};
use scry_addons::Meta;
use std::path::Path;

fn meta(id: &str, title: &str, year: &str) -> Meta {
    serde_json::from_value(serde_json::json!({"id": id,"type":"movie","name":title,"year":year}))
        .unwrap()
}

#[test]
fn folders_supply_show_and_season_for_numeric_episodes() {
    let hints = infer(Path::new("Severance/Season 01/01.mkv"));
    assert_eq!(hints.title, "Severance");
    assert_eq!((hints.season, hints.episode), (Some(1), Some(1)));
    let hints = infer(Path::new("Severance/Season 01/Severance.S01E02.1080p.mkv"));
    assert_eq!(hints.title, "Severance");
    assert_eq!((hints.season, hints.episode), (Some(1), Some(2)));
    let hints = infer(Path::new("Severance/Season01/03 - In Perpetuity.mkv"));
    assert_eq!(hints.title, "Severance");
    assert_eq!((hints.season, hints.episode), (Some(1), Some(3)));
}

#[test]
fn movie_release_tags_and_year_do_not_become_title() {
    let hints = infer(Path::new(
        "Movies/Arrival (2016)/arrival.2016.1080p.BluRay.x264.mkv",
    ));
    assert_eq!(hints.title, "arrival");
    assert_eq!(hints.year, Some(2016));
    let hints = infer(Path::new("Movies/1917.2019.1080p.mkv"));
    assert_eq!(hints.title, "1917");
    assert_eq!(hints.year, Some(2019));
}

#[test]
fn explicit_id_is_found_after_unrelated_double_t_letters() {
    assert_eq!(
        super::local_media::imdb_id("Little.Film.[tt1234567].mkv").as_deref(),
        Some("tt1234567")
    );
    assert!(super::local_media::imdb_id("tt123456789012").is_none());
}

#[test]
fn conflicting_and_multi_episode_numbering_never_auto_links() {
    assert!(infer(Path::new("Show/Season 2/Show.S01E02.mkv")).conflict);
    assert!(infer(Path::new("Show/Show.S01E01E02.mkv")).conflict);
    assert!(infer(Path::new("Show/Show.S01E01-E02.mkv")).conflict);
    assert!(infer(Path::new("Show/Show.S01E01-02.mkv")).conflict);
    assert!(infer(Path::new("Show/Show.S01E01.E02.mkv")).conflict);
}

#[test]
fn scan_indexes_multiple_roots_and_preserves_availability_on_cancel() -> anyhow::Result<()> {
    use super::local_media::scan;
    use scry_library::Library;
    use std::sync::atomic::AtomicBool;
    let folder = tempfile::tempdir()?;
    let first = folder.path().join("movies");
    let second = folder.path().join("shows");
    std::fs::create_dir(&first)?;
    std::fs::create_dir(&second)?;
    std::fs::write(first.join("Arrival.2016.mkv"), b"movie")?;
    std::fs::write(
        first.join("Arrival.2016.nfo"),
        b"https://www.imdb.com/title/tt2543164/",
    )?;
    std::fs::write(first.join("notes.txt"), b"not video")?;
    std::fs::write(second.join("Show.S01E01.mp4"), b"episode")?;
    let db = Library::memory()?;
    db.add_local_root(&first.to_string_lossy())?;
    db.add_local_root(&second.to_string_lossy())?;
    let roots = db.local_roots()?;
    for root in &roots {
        assert_eq!(scan(&db, root, &AtomicBool::new(false))?, 1);
    }
    let files = db.local_files(0, false)?;
    assert_eq!(files.len(), 2);
    let movie = files
        .iter()
        .find(|file| file.relative_path.ends_with("mkv"))
        .unwrap();
    let hints: Hints = serde_json::from_str(&movie.hints)?;
    assert_eq!(hints.explicit_id.as_deref(), Some("tt2543164"));
    std::fs::remove_file(first.join("Arrival.2016.mkv"))?;
    assert!(scan(&db, &roots[0], &AtomicBool::new(true)).is_err());
    assert!(db.local_file(movie.id)?.available);
    assert_eq!(scan(&db, &roots[0], &AtomicBool::new(false))?, 0);
    assert!(!db.local_file(movie.id)?.available);
    assert!(
        db.local_file(
            files
                .iter()
                .find(|file| file.root_id == roots[1].id)
                .unwrap()
                .id
        )?
        .available
    );
    Ok(())
}

#[test]
fn a_show_root_supplies_the_title_for_season_subfolders() -> anyhow::Result<()> {
    let folder = tempfile::tempdir()?;
    let root_path = folder.path().join("Severance");
    std::fs::create_dir_all(root_path.join("Season 1"))?;
    std::fs::write(root_path.join("Season 1").join("01.mkv"), b"episode")?;
    std::fs::write(root_path.join("tvshow.nfo"), b"<imdbid>tt11280740</imdbid>")?;
    let db = scry_library::Library::memory()?;
    db.add_local_root(&root_path.to_string_lossy())?;
    let root = db.local_roots()?.remove(0);
    super::local_media::scan(&db, &root, &std::sync::atomic::AtomicBool::new(false))?;
    let file = db.local_files(0, false)?.remove(0);
    let hints: Hints = serde_json::from_str(&file.hints)?;
    assert_eq!(hints.title, "Severance");
    assert_eq!((hints.season, hints.episode), (Some(1), Some(1)));
    assert_eq!(hints.explicit_id.as_deref(), Some("tt11280740"));
    Ok(())
}

#[test]
fn remakes_require_year_and_explicit_ids_do_not_override_conflicts() {
    let mut hints = Hints {
        title: "The Thing".into(),
        ..Hints::default()
    };
    let candidates = vec![
        meta("tt1", "The Thing", "1982"),
        meta("tt2", "The Thing", "2011"),
    ];
    assert!(select_match(&hints, &candidates).is_none());
    hints.year = Some(1982);
    assert_eq!(select_match(&hints, &candidates), Some(0));
    hints.conflict = true;
    hints.explicit_id = Some("tt1".into());
    assert!(select_match(&hints, &candidates).is_none());
}

#[test]
fn ignores_quality_tokens_and_supports_x_episode_numbers() {
    let hints = infer(Path::new("Show/Show.2x03.720p.mkv"));
    assert_eq!(hints.title, "Show");
    assert_eq!((hints.season, hints.episode), (Some(2), Some(3)));
    let hints = infer(Path::new("Film/film.1080p.mkv"));
    assert_eq!(hints.season, None);
    assert_eq!(hints.episode, None);
}

#[test]
fn samples_and_trailers_are_not_automatic_movie_sources() {
    assert!(infer(Path::new("Arrival.2016.sample.mkv")).conflict);
    assert!(infer(Path::new("Arrival (2016)/Extras/Trailer.mkv")).conflict);
}
