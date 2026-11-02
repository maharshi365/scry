use super::*;
use diesel::connection::SimpleConnection;

#[test]
fn empty_dashboard_has_zero_totals_and_no_rankings() -> Result<()> {
    let db = Library::memory()?;
    let stats = db.nerd_stats(StatsPeriod::AllTime)?;
    assert!(stats.watch_seconds.abs() < f64::EPSILON);
    assert!(stats.movie_seconds.abs() < f64::EPSILON);
    assert!(stats.tv_seconds.abs() < f64::EPSILON);
    assert_eq!(stats.saved_titles, 0);
    assert_eq!(stats.saved_watched, 0);
    assert_eq!(stats.series_completed, 0);
    assert_eq!(stats.unfinished_shows, 0);
    assert_eq!(stats.unknown_runtimes, 0);
    assert_eq!(stats.recorded_completions, 0);
    assert!(stats.recorded_seconds.abs() < f64::EPSILON);
    assert!(stats.rewatch_seconds.abs() < f64::EPSILON);
    assert_eq!(stats.session_count, 0);
    assert!(stats.longest_session_seconds.abs() < f64::EPSILON);
    assert_eq!(stats.most_session_episodes, 0);
    assert!(stats.top_titles.is_empty());
    Ok(())
}

#[test]
fn dashboard_aggregates_history_credit_sessions_and_series_completion() -> Result<()> {
    let db = Library::memory()?;
    db.immediate_transaction(|connection| {
        connection.batch_execute(
            "INSERT INTO media(media_id,namespace,kind,external_id,title,parent_id) VALUES
                (1,'test','movie','movie','Movie',NULL),
                (2,'test','series','complete','Complete',NULL),
                (3,'test','episode','one','One',2),
                (4,'test','episode','special','Special',2),
                (5,'test','series','unfinished','Unfinished',NULL),
                (6,'test','episode','future','Future',5),
                (7,'test','movie','unknown','Unknown',NULL);
            INSERT INTO episode_releases(media_id,released_at,season,episode) VALUES
                (3,NULL,1,1),(4,unixepoch()+86400,0,1),(6,unixepoch()+86400,1,1);
            INSERT INTO watched VALUES (1,1),(3,1),(7,1);
            INSERT INTO watchlist(media_id) VALUES (1),(2),(5);
            INSERT INTO media_runtime(media_id,seconds,watched_at) VALUES
                (1,1000,1),(3,600,unixepoch());
            INSERT INTO stats_titles(media_id,seconds,rewatch_seconds,completions) VALUES
                (1,200,50,1),(3,100,0,1);
            INSERT INTO stats_hours VALUES (0,1,200),(unixepoch()/3600*3600,3,100);
            INSERT INTO stats_sessions(started_at,last_at,seconds,episodes) VALUES
                (1,2,200,0),(3,4,100,1);",
        )?;
        Ok(())
    })?;
    let stats = db.nerd_stats(StatsPeriod::AllTime)?;
    assert!((stats.watch_seconds - 1600.0).abs() < f64::EPSILON);
    assert!((stats.movie_seconds - 1000.0).abs() < f64::EPSILON);
    assert!((stats.tv_seconds - 600.0).abs() < f64::EPSILON);
    assert_eq!((stats.movies_watched, stats.episodes_watched), (2, 1));
    assert_eq!((stats.series_completed, stats.unfinished_shows), (1, 1));
    assert_eq!((stats.saved_titles, stats.saved_watched), (3, 2));
    assert_eq!(stats.unknown_runtimes, 1);
    assert_eq!(stats.recorded_completions, 2);
    assert!((stats.recorded_seconds - 300.0).abs() < f64::EPSILON);
    assert!((stats.rewatch_seconds - 50.0).abs() < f64::EPSILON);
    assert_eq!(stats.session_count, 2);
    assert!((stats.longest_session_seconds - 200.0).abs() < f64::EPSILON);
    assert_eq!(stats.most_session_episodes, 1);
    assert_eq!(stats.top_titles.len(), 2);
    assert_eq!(stats.top_titles[0].title, "Movie");
    assert!((stats.top_titles[0].seconds - 1000.0).abs() < f64::EPSILON);
    assert_eq!(stats.top_titles[1].title, "Complete");
    for period in [StatsPeriod::Week, StatsPeriod::Month, StatsPeriod::Year] {
        let recent = db.nerd_stats(period)?;
        assert!((recent.watch_seconds - 600.0).abs() < f64::EPSILON);
        assert!(recent.movie_seconds.abs() < f64::EPSILON);
        assert_eq!(recent.saved_watched, 2);
        assert_eq!(recent.top_titles.len(), 1);
        assert_eq!(recent.top_titles[0].title, "Complete");
    }
    Ok(())
}
