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

#[derive(QueryableByName)]
struct PlaybackTotals {
    #[diesel(sql_type = Double)]
    total: f64,
    #[diesel(sql_type = Double)]
    movies: f64,
    #[diesel(sql_type = Double)]
    tv: f64,
}

#[derive(QueryableByName)]
struct LibraryTotals {
    #[diesel(sql_type = BigInt)]
    movies_watched: i64,
    #[diesel(sql_type = BigInt)]
    episodes_watched: i64,
    #[diesel(sql_type = BigInt)]
    series_completed: i64,
    #[diesel(sql_type = BigInt)]
    unfinished_shows: i64,
    #[diesel(sql_type = BigInt)]
    saved_titles: i64,
    #[diesel(sql_type = BigInt)]
    saved_watched: i64,
    #[diesel(sql_type = BigInt)]
    unknown_runtimes: i64,
}

#[derive(QueryableByName)]
struct RecordedTotals {
    #[diesel(sql_type = BigInt)]
    completions: i64,
    #[diesel(sql_type = Double)]
    seconds: f64,
    #[diesel(sql_type = Double)]
    rewatch_seconds: f64,
}

#[derive(QueryableByName)]
struct SessionTotals {
    #[diesel(sql_type = BigInt)]
    count: i64,
    #[diesel(sql_type = Double)]
    longest_seconds: f64,
    #[diesel(sql_type = BigInt)]
    most_episodes: i64,
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

pub(crate) fn validate(connection: &mut SqliteConnection) -> Result<()> {
    // Resolve every stats column at startup without scanning history.
    count(
        connection,
        "SELECT COUNT(*) AS value FROM (SELECT t.id,t.started_at,r.media_id,r.seconds,r.watched_at,p.media_id,p.seconds,p.rewatch_seconds,p.completions,p.run_seconds,p.run_completed,h.hour,h.media_id,h.seconds,s.id,s.started_at,s.last_at,s.seconds,s.episodes,a.id,a.unlocked,a.historical,a.unlocked_at,c.media_id,c.recorded_seconds,c.credit,c.watched_at FROM stats_tracking t,media_runtime r,stats_titles p,stats_hours h,stats_sessions s,achievement_checks a,stats_counted c LIMIT 0)",
    )?;
    Ok(())
}

// Called inside the same transaction as resume/watched state. No samples are
// stored: one bucket per title/hour, one total per title, one row per session.
pub(crate) fn record(
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

pub(crate) fn runtime(connection: &mut SqliteConnection, id: i64, seconds: f64) -> Result<()> {
    diesel::sql_query("INSERT INTO media_runtime(media_id,seconds) VALUES (?,?) ON CONFLICT(media_id) DO UPDATE SET seconds=excluded.seconds")
        .bind::<BigInt,_>(id).bind::<Double,_>(seconds).execute(connection)?;
    Ok(())
}

pub(crate) fn watched_at(
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

pub(crate) fn finish_run(connection: &mut SqliteConnection, id: i64) -> Result<()> {
    diesel::sql_query("UPDATE stats_titles SET run_seconds=0,run_completed=0 WHERE media_id=?")
        .bind::<BigInt, _>(id)
        .execute(connection)?;
    Ok(())
}

impl Library {
    /// Read watched movies with missing runtimes in stable identity order.
    /// The cursor remains valid when earlier movies receive runtimes.
    /// # Errors
    /// Returns an error for an invalid cursor, page size, or database read.
    pub fn movies_missing_runtime(
        &self,
        after: Option<&crate::MediaKey>,
        limit: u32,
    ) -> Result<Vec<crate::Media>> {
        let page = crate::Page::new(limit, 0)?;
        anyhow::ensure!(
            after.is_none_or(|key| key.kind == crate::MediaKind::Movie),
            "invalid movie runtime cursor"
        );
        self.with_connection(|connection| {
            let records = diesel::sql_query("SELECT m.* FROM media m JOIN effective_watched w USING(media_id) LEFT JOIN media_runtime r USING(media_id) WHERE m.kind='movie' AND w.watched=1 AND r.seconds IS NULL AND (m.namespace,m.external_id)>(?,?) ORDER BY m.namespace,m.external_id LIMIT ?")
                .bind::<Text, _>(after.map_or("", |key| key.namespace.as_str()))
                .bind::<Text, _>(after.map_or("", |key| key.id.as_str()))
                .bind::<BigInt, _>(i64::from(page.limit))
                .load::<crate::catalog::media::MediaRecord>(connection)?;
            crate::catalog::media::hydrate(connection, records)
        })
    }

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
        self.immediate_transaction(|connection| {
            for (key, seconds) in runtimes {
                if let Some(id) = crate::storage::schema::media::table
                    .filter(crate::storage::schema::media::namespace.eq(&key.namespace))
                    .filter(crate::storage::schema::media::kind.eq(key.kind.as_str()))
                    .filter(crate::storage::schema::media::external_id.eq(&key.id))
                    .select(crate::storage::schema::media::media_id)
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
        self.transaction(|connection| {
            let start = period.start();
            let history = include_str!("history.sql").replace("{start}", start);
            let playback = diesel::sql_query(format!("{history} SELECT COALESCE(SUM(seconds),0.0) AS total, COALESCE(SUM(CASE WHEN kind='movie' THEN seconds ELSE 0.0 END),0.0) AS movies, COALESCE(SUM(CASE WHEN kind='episode' THEN seconds ELSE 0.0 END),0.0) AS tv FROM history"))
                .get_result::<PlaybackTotals>(connection)?;
            let complete = crate::playback::watch_status::completion_sql("m.media_id", "unixepoch()");
            let library = diesel::sql_query(
                include_str!("library_totals.sql").replace("{complete}", &complete),
            )
            .get_result::<LibraryTotals>(connection)?;
            let recorded = diesel::sql_query("SELECT COALESCE(SUM(completions),0) AS completions, COALESCE(SUM(seconds),0.0) AS seconds, COALESCE(SUM(rewatch_seconds),0.0) AS rewatch_seconds FROM stats_titles")
                .get_result::<RecordedTotals>(connection)?;
            let sessions = diesel::sql_query("SELECT COUNT(*) AS count, COALESCE(MAX(seconds),0.0) AS longest_seconds, COALESCE(MAX(episodes),0) AS most_episodes FROM stats_sessions")
                .get_result::<SessionTotals>(connection)?;
            Ok(NerdStats {
                started_at: count(connection, "SELECT started_at AS value FROM stats_tracking WHERE id=1")?,
                watch_seconds: playback.total,
                movie_seconds: playback.movies,
                tv_seconds: playback.tv,
                movies_watched: library.movies_watched,
                episodes_watched: library.episodes_watched,
                series_completed: library.series_completed,
                unfinished_shows: library.unfinished_shows,
                recorded_completions: recorded.completions,
                recorded_seconds: recorded.seconds,
                saved_titles: library.saved_titles,
                saved_watched: library.saved_watched,
                session_count: sessions.count,
                rewatch_seconds: recorded.rewatch_seconds,
                unknown_runtimes: library.unknown_runtimes,
                longest_session_seconds: sessions.longest_seconds,
                most_session_episodes: sessions.most_episodes,
                top_titles: diesel::sql_query(format!("{history} SELECT COALESCE(parent.title,m.title) AS title,SUM(h.seconds) AS seconds FROM history h JOIN media m ON m.media_id=h.media_id LEFT JOIN media parent ON parent.media_id=m.parent_id GROUP BY COALESCE(m.parent_id,m.media_id) ORDER BY seconds DESC, title LIMIT 5")).load(connection)?,
            })
        })
    }
}

#[cfg(test)]
mod tests;
