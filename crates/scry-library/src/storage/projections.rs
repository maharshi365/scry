//! Transactional read models. Source changes enqueue identities, not full rebuilds.
use anyhow::Result;
use diesel::{connection::SimpleConnection, prelude::*};
use std::fmt::Write as _;

pub(super) fn install(connection: &mut SqliteConnection) -> Result<()> {
    connection.batch_execute(include_str!("projections.sql"))?;
    for (table, domain) in [
        ("media", "library"),
        ("watched", "library"),
        ("episode_releases", "library"),
        ("media_runtime", "library"),
        ("watchlist", "library"),
        ("stats_titles", "playback"),
        ("stats_sessions", "playback"),
        ("stats_hours", "hours"),
    ] {
        for event in ["INSERT", "UPDATE", "DELETE"] {
            let row = if event == "DELETE" { "OLD" } else { "NEW" };
            let mut body = format!("UPDATE source_revisions SET {domain}={domain}+1 WHERE id=1;");
            if matches!(
                table,
                "media" | "watched" | "media_runtime" | "stats_titles"
            ) {
                write!(
                    body,
                    "INSERT INTO dirty_titles SELECT {row}.media_id WHERE NOT EXISTS(SELECT 1 FROM dirty_titles WHERE media_id={row}.media_id);"
                )?;
            }
            if matches!(table, "media" | "watched" | "episode_releases") {
                if table == "media" {
                    let current = [row];
                    let references = if event == "UPDATE" {
                        &["OLD", "NEW"][..]
                    } else {
                        &current[..]
                    };
                    for reference in references {
                        write!(
                            body,
                            "INSERT INTO dirty_series SELECT {reference}.parent_id WHERE {reference}.parent_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM dirty_series WHERE media_id={reference}.parent_id); INSERT INTO dirty_series SELECT {reference}.media_id WHERE {reference}.kind='series' AND NOT EXISTS(SELECT 1 FROM dirty_series WHERE media_id={reference}.media_id);"
                        )?;
                    }
                } else {
                    write!(
                        body,
                        "INSERT INTO dirty_series SELECT parent_id FROM media WHERE media_id={row}.media_id AND parent_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM dirty_series WHERE media_id=media.parent_id);"
                    )?;
                }
            }
            if table == "stats_hours" {
                if event != "INSERT" {
                    body.push_str("UPDATE stats_days SET seconds=MAX(0.0,seconds-OLD.seconds) WHERE day=OLD.hour/86400; DELETE FROM stats_days WHERE day=OLD.hour/86400 AND seconds=0;");
                }
                if event != "DELETE" {
                    body.push_str("INSERT INTO stats_days(day,seconds) VALUES(NEW.hour/86400,NEW.seconds) ON CONFLICT(day) DO UPDATE SET seconds=seconds+excluded.seconds;");
                }
            }
            // Metadata-only changes do not invalidate watched state or analytics.
            let condition = match (table, event) {
                ("media", "UPDATE") => {
                    "WHEN OLD.kind IS NOT NEW.kind OR OLD.parent_id IS NOT NEW.parent_id"
                }
                ("watched", "UPDATE") => "WHEN OLD.watched IS NOT NEW.watched",
                ("media_runtime", "UPDATE") => {
                    "WHEN OLD.seconds IS NOT NEW.seconds OR OLD.watched_at IS NOT NEW.watched_at"
                }
                ("stats_titles", "UPDATE") => {
                    "WHEN OLD.seconds IS NOT NEW.seconds OR OLD.completions IS NOT NEW.completions"
                }
                ("episode_releases", "UPDATE") => {
                    "WHEN OLD.released_at IS NOT NEW.released_at OR OLD.season IS NOT NEW.season OR OLD.episode IS NOT NEW.episode"
                }
                _ => "",
            };
            connection.batch_execute(&format!("CREATE TRIGGER IF NOT EXISTS projection_{table}_{event} AFTER {event} ON {table} {condition} BEGIN {body} END;"))?;
        }
    }
    for (event, row, sign) in [("INSERT", "NEW", "+"), ("DELETE", "OLD", "-")] {
        connection.batch_execute(&format!(
            "CREATE TRIGGER IF NOT EXISTS watch_totals_{event} AFTER {event} ON watch_facts BEGIN
             UPDATE watch_totals SET
                movie_seconds=movie_seconds{sign}CASE WHEN {row}.kind='movie' THEN {row}.recorded_seconds+{row}.credit ELSE 0 END,
                tv_seconds=tv_seconds{sign}CASE WHEN {row}.kind='episode' THEN {row}.recorded_seconds+{row}.credit ELSE 0 END,
                movies_watched=movies_watched{sign}({row}.kind='movie' AND {row}.watched=1),
                episodes_watched=episodes_watched{sign}({row}.kind='episode' AND {row}.watched=1),
                unknown_runtimes=unknown_runtimes{sign}{row}.unknown_runtime WHERE id=1; END;"
        ))?;
    }
    // Rebuild only on the schema upgrade. The queue is durable across interruption.
    connection.batch_execute(
        "INSERT OR IGNORE INTO dirty_titles SELECT media_id FROM media;
         INSERT OR IGNORE INTO dirty_series SELECT media_id FROM media WHERE kind='series';
         DELETE FROM stats_days;
         INSERT INTO stats_days SELECT hour/86400,SUM(seconds) FROM stats_hours GROUP BY hour/86400;",
    )?;
    refresh(connection)
}

pub(super) fn refresh(connection: &mut SqliteConnection) -> Result<()> {
    #[derive(QueryableByName)]
    struct Pending {
        #[diesel(sql_type = diesel::sql_types::Bool)]
        pending: bool,
    }
    if !diesel::sql_query("SELECT EXISTS(SELECT 1 FROM dirty_titles) OR EXISTS(SELECT 1 FROM dirty_series) AS pending")
        .get_result::<Pending>(connection)?.pending {
        return Ok(());
    }
    // CROSS JOIN keeps each queue as the outer loop, even without ANALYZE.
    // Otherwise SQLite can scan all media to refresh a single dirty title.
    connection.batch_execute(
        "DELETE FROM watch_facts WHERE media_id IN (SELECT media_id FROM dirty_titles);
         INSERT INTO watch_facts
         SELECT m.media_id,m.kind,COALESCE(w.watched,0),COALESCE(t.seconds,0.0),
            CASE WHEN w.watched=1 THEN MAX(COALESCE(r.seconds,0.0)-COALESCE(t.seconds,0.0),0.0) ELSE 0.0 END,
            r.watched_at,COALESCE(w.watched,0)=1 AND r.seconds IS NULL
         FROM dirty_titles d CROSS JOIN media m USING(media_id)
         LEFT JOIN watched w USING(media_id) LEFT JOIN stats_titles t USING(media_id)
         LEFT JOIN media_runtime r USING(media_id) WHERE m.kind IN ('movie','episode');
         DELETE FROM dirty_titles;
         DELETE FROM series_facts WHERE media_id IN (SELECT media_id FROM dirty_series);
         INSERT INTO series_facts
         SELECT e.parent_id,COUNT(*),SUM(COALESCE(w.watched,0)),MAX(r.released_at)
         FROM dirty_series d CROSS JOIN media e ON e.parent_id=d.media_id
         JOIN episode_releases r ON r.media_id=e.media_id
         LEFT JOIN watched w ON w.media_id=e.media_id
         WHERE r.season IS NULL OR r.season!=0 GROUP BY e.parent_id;
         DELETE FROM dirty_series;",
    )?;
    Ok(())
}

pub(super) fn validate(connection: &mut SqliteConnection) -> Result<()> {
    connection.batch_execute(
        "SELECT media_id FROM dirty_titles LIMIT 0;
         SELECT media_id FROM dirty_series LIMIT 0;
         SELECT library,playback,hours FROM source_revisions LIMIT 0;
         SELECT movie_seconds,tv_seconds,movies_watched,episodes_watched,unknown_runtimes FROM watch_totals LIMIT 0;
         SELECT media_id,kind,watched,recorded_seconds,credit,watched_at,unknown_runtime FROM watch_facts LIMIT 0;
         SELECT media_id,total,watched,last_release FROM series_facts LIMIT 0;
         SELECT day,seconds FROM stats_days LIMIT 0;",
    )?;
    Ok(())
}
