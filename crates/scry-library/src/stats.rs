use anyhow::Result;
use diesel::{
    prelude::*,
    sql_types::{BigInt, Double, Text},
};

use crate::Library;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatsPeriod {
    Week,
    Month,
    Year,
    #[default]
    AllTime,
}

impl StatsPeriod {
    fn start(self) -> &'static str {
        match self {
            Self::Week => "CAST(strftime('%s',date('now','weekday 0','-6 days')) AS INTEGER)",
            Self::Month => "CAST(strftime('%s',date('now','start of month')) AS INTEGER)",
            Self::Year => "CAST(strftime('%s',date('now','start of year')) AS INTEGER)",
            Self::AllTime => "0",
        }
    }
}

#[derive(Clone, Debug, QueryableByName)]
pub struct TopTitle {
    #[diesel(sql_type = Text)]
    pub title: String,
    #[diesel(sql_type = Double)]
    pub seconds: f64,
}

#[derive(Clone, Debug, Default)]
pub struct NerdStats {
    pub started_at: i64,
    pub watch_seconds: f64,
    pub movie_seconds: f64,
    pub tv_seconds: f64,
    pub movies_watched: i64,
    pub episodes_watched: i64,
    pub series_completed: i64,
    pub unfinished_shows: i64,
    pub recorded_completions: i64,
    pub recorded_seconds: f64,
    pub saved_titles: i64,
    pub saved_watched: i64,
    pub session_count: i64,
    pub rewatch_seconds: f64,
    pub unknown_runtimes: i64,
    pub longest_session_seconds: f64,
    pub most_session_episodes: i64,
    pub top_titles: Vec<TopTitle>,
}

#[derive(QueryableByName)]
struct Timestamp {
    #[diesel(sql_type = BigInt)]
    value: i64,
}

#[derive(QueryableByName)]
struct Number {
    #[diesel(sql_type = Double)]
    value: f64,
}

pub(super) fn number(connection: &mut SqliteConnection, query: &str) -> Result<f64> {
    Ok(diesel::sql_query(query)
        .get_result::<Number>(connection)?
        .value)
}

pub(super) fn count(connection: &mut SqliteConnection, query: &str) -> Result<i64> {
    Ok(diesel::sql_query(query)
        .get_result::<Timestamp>(connection)?
        .value)
}

pub(super) fn validate(connection: &mut SqliteConnection) -> Result<()> {
    // Resolve every stats column at startup without scanning history.
    count(
        connection,
        "SELECT COUNT(*) AS value FROM (SELECT t.id,t.started_at,r.media_id,r.seconds,r.watched_at,p.media_id,p.seconds,p.rewatch_seconds,p.completions,p.run_seconds,p.run_completed,h.hour,h.media_id,h.seconds,s.id,s.started_at,s.last_at,s.seconds,s.episodes,a.id,a.unlocked,a.historical,a.unlocked_at,c.media_id,c.recorded_seconds,c.credit,c.watched_at FROM stats_tracking t,media_runtime r,stats_titles p,stats_hours h,stats_sessions s,achievement_checks a,stats_counted c LIMIT 0)",
    )?;
    Ok(())
}

// Called inside the same transaction as resume/watched state. No samples are
// stored: one bucket per title/hour, one total per title, one row per session.
pub(super) fn record(
    connection: &mut SqliteConnection,
    id: i64,
    delta: f64,
    duration: f64,
) -> Result<()> {
    if delta == 0.0 {
        return Ok(());
    }
    let now = count(connection, "SELECT unixepoch() AS value")?;
    let completed = diesel::sql_query("SELECT CAST(COALESCE((SELECT run_completed=0 AND run_seconds+?>? FROM stats_titles WHERE media_id=?), ?>?) AS INTEGER) AS value")
        .bind::<Double,_>(delta).bind::<Double,_>(duration * crate::WATCHED_THRESHOLD)
        .bind::<BigInt,_>(id).bind::<Double,_>(delta).bind::<Double,_>(duration * crate::WATCHED_THRESHOLD)
        .get_result::<Timestamp>(connection)?.value;
    diesel::sql_query("INSERT INTO stats_titles(media_id,seconds,completions,run_seconds,run_completed) VALUES (?,?,?,?,?) ON CONFLICT(media_id) DO UPDATE SET rewatch_seconds=rewatch_seconds+CASE WHEN completions>run_completed THEN excluded.seconds ELSE 0 END, seconds=seconds+excluded.seconds, completions=completions+excluded.completions, run_seconds=run_seconds+excluded.run_seconds, run_completed=MAX(run_completed,excluded.run_completed)")
        .bind::<BigInt,_>(id).bind::<Double,_>(delta).bind::<BigInt,_>(completed)
        .bind::<Double,_>(delta).bind::<BigInt,_>(completed).execute(connection)?;
    diesel::sql_query("INSERT INTO stats_hours(hour,media_id,seconds) VALUES (?,?,?) ON CONFLICT(hour,media_id) DO UPDATE SET seconds=seconds+excluded.seconds")
        .bind::<BigInt,_>(now / 3600 * 3600).bind::<BigInt,_>(id).bind::<Double,_>(delta).execute(connection)?;
    let episodes = diesel::sql_query(
        "SELECT CAST(kind='episode' AS INTEGER)*? AS value FROM media WHERE media_id=?",
    )
    .bind::<BigInt, _>(completed)
    .bind::<BigInt, _>(id)
    .get_result::<Timestamp>(connection)?
    .value;
    let session = count(
        connection,
        "SELECT COALESCE((SELECT CASE WHEN last_at>=unixepoch()-1800 THEN id ELSE 0 END FROM stats_sessions ORDER BY id DESC LIMIT 1),0) AS value",
    )?;
    if session == 0 {
        diesel::sql_query(
            "INSERT INTO stats_sessions(started_at,last_at,seconds,episodes) VALUES (?,?,?,?)",
        )
        .bind::<BigInt, _>(now)
        .bind::<BigInt, _>(now)
        .bind::<Double, _>(delta)
        .bind::<BigInt, _>(episodes)
        .execute(connection)?;
    } else {
        diesel::sql_query("UPDATE stats_sessions SET last_at=?, seconds=seconds+?, episodes=episodes+? WHERE id=?")
            .bind::<BigInt,_>(now).bind::<Double,_>(delta).bind::<BigInt,_>(episodes).bind::<BigInt,_>(session).execute(connection)?;
    }
    Ok(())
}

pub(super) fn runtime(connection: &mut SqliteConnection, id: i64, seconds: f64) -> Result<()> {
    diesel::sql_query("INSERT INTO media_runtime(media_id,seconds) VALUES (?,?) ON CONFLICT(media_id) DO UPDATE SET seconds=excluded.seconds")
        .bind::<BigInt,_>(id).bind::<Double,_>(seconds).execute(connection)?;
    Ok(())
}

pub(super) fn watched_at(
    connection: &mut SqliteConnection,
    id: i64,
    watched: bool,
    was_watched: bool,
) -> Result<()> {
    if watched && !was_watched {
        diesel::sql_query("INSERT INTO media_runtime(media_id,watched_at) VALUES (?,unixepoch()) ON CONFLICT(media_id) DO UPDATE SET watched_at=excluded.watched_at")
            .bind::<BigInt,_>(id).execute(connection)?;
    } else if !watched {
        diesel::sql_query("UPDATE media_runtime SET watched_at=NULL WHERE media_id=?")
            .bind::<BigInt, _>(id)
            .execute(connection)?;
    }
    Ok(())
}

pub(super) fn finish_run(connection: &mut SqliteConnection, id: i64) -> Result<()> {
    diesel::sql_query("UPDATE stats_titles SET run_seconds=0,run_completed=0 WHERE media_id=?")
        .bind::<BigInt, _>(id)
        .execute(connection)?;
    Ok(())
}

impl Library {
    /// Store known runtimes without changing watched state. Missing titles are skipped.
    /// # Errors
    /// Returns an error for invalid runtimes, oversized batches, or database writes.
    pub fn set_known_runtimes(&mut self, runtimes: &[(crate::MediaKey, f64)]) -> Result<()> {
        anyhow::ensure!(
            runtimes.len() <= 50_000
                && runtimes
                    .iter()
                    .all(|(key, seconds)| key.kind != crate::MediaKind::Series
                        && seconds.is_finite()
                        && *seconds > 0.0),
            "invalid runtime batch"
        );
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                for (key, seconds) in runtimes {
                    if let Some(id) = crate::schema::media::table
                        .filter(crate::schema::media::namespace.eq(&key.namespace))
                        .filter(crate::schema::media::kind.eq(key.kind.as_str()))
                        .filter(crate::schema::media::external_id.eq(&key.id))
                        .select(crate::schema::media::media_id)
                        .first::<i64>(connection)
                        .optional()?
                    {
                        runtime(connection, id, *seconds)?;
                    }
                }
                Ok(())
            })
    }
    /// Read bounded display data. Playback filters use UTC calendar boundaries;
    /// library completion counts always describe current saved state.
    /// # Errors
    /// Returns an error if statistics cannot be read.
    pub fn nerd_stats(&self, period: StatsPeriod) -> Result<NerdStats> {
        let connection = &mut *self.connection.borrow_mut();
        connection.transaction(|connection| {
            let start = period.start();
            let credit_filter = if period == StatsPeriod::AllTime { "1".into() } else { format!("c.watched_at>={start}") };
            let seconds = |kind: &str| format!("SELECT COALESCE((SELECT SUM(h.seconds) FROM stats_hours h JOIN media m ON m.media_id=h.media_id WHERE h.hour>={start} AND {kind}),0.0)+COALESCE((SELECT SUM(c.credit) FROM stats_counted c JOIN media m ON m.media_id=c.media_id WHERE {credit_filter} AND {kind}),0.0) AS value");
            Ok(NerdStats {
                started_at: count(connection, "SELECT started_at AS value FROM stats_tracking WHERE id=1")?,
                watch_seconds: number(connection, &seconds("1"))?,
                movie_seconds: number(connection, &seconds("m.kind='movie'"))?,
                tv_seconds: number(connection, &seconds("m.kind='episode'"))?,
                movies_watched: count(connection, "SELECT COUNT(*) AS value FROM media m JOIN effective_watched w USING(media_id) WHERE m.kind='movie' AND w.watched=1")?,
                episodes_watched: count(connection, "SELECT COUNT(*) AS value FROM media m JOIN effective_watched w USING(media_id) WHERE m.kind='episode' AND w.watched=1")?,
                series_completed: count(connection, "SELECT COUNT(*) AS value FROM media m JOIN effective_watched w USING(media_id) WHERE m.kind='series' AND w.watched=1")?,
                unfinished_shows: count(connection, "SELECT COUNT(*) AS value FROM media m JOIN watchlist l USING(media_id) JOIN effective_watched w USING(media_id) WHERE m.kind='series' AND w.watched=0")?,
                recorded_completions: count(connection, "SELECT COALESCE(SUM(completions),0) AS value FROM stats_titles")?,
                recorded_seconds: number(connection, "SELECT COALESCE(SUM(seconds),0.0) AS value FROM stats_titles")?,
                saved_titles: count(connection, "SELECT COUNT(*) AS value FROM watchlist")?,
                saved_watched: count(connection, "SELECT COUNT(*) AS value FROM watchlist l JOIN effective_watched w USING(media_id) WHERE w.watched=1")?,
                session_count: count(connection, "SELECT COUNT(*) AS value FROM stats_sessions")?,
                rewatch_seconds: number(connection, "SELECT COALESCE(SUM(rewatch_seconds),0.0) AS value FROM stats_titles")?,
                unknown_runtimes: count(connection, "SELECT COUNT(*) AS value FROM media m JOIN effective_watched w USING(media_id) LEFT JOIN media_runtime r USING(media_id) WHERE m.kind IN ('movie','episode') AND w.watched=1 AND r.seconds IS NULL")?,
                longest_session_seconds: number(connection, "SELECT COALESCE(MAX(seconds),0.0) AS value FROM stats_sessions")?,
                most_session_episodes: count(connection, "SELECT COALESCE(MAX(episodes),0) AS value FROM stats_sessions")?,
                top_titles: diesel::sql_query(format!("SELECT COALESCE(parent.title,m.title) AS title,SUM(h.seconds) AS seconds FROM (SELECT media_id,seconds FROM stats_hours WHERE hour>={start} UNION ALL SELECT media_id,credit AS seconds FROM stats_counted c WHERE {credit_filter} AND credit>0) h JOIN media m ON m.media_id=h.media_id LEFT JOIN media parent ON parent.media_id=m.parent_id GROUP BY COALESCE(m.parent_id,m.media_id) ORDER BY seconds DESC, title LIMIT 5")).load(connection)?,
            })
        })
    }
}
