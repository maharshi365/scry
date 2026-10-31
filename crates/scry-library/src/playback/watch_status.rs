//! Single owner of release eligibility, series counts, progress, and completion.
//! The database view uses the same summary query as explicit-time status reads.
use anyhow::{Result, ensure};
use diesel::{connection::SimpleConnection, prelude::*, sql_types::BigInt};

use crate::{Library, MediaKey, MediaKind, Progress, SeriesProgress, catalog::media::media_id};

const ELIGIBLE: &str = "COALESCE(r.released_at <= {now}, 0)";
const RETAINED: &str = "r.released_at IS NULL OR r.released_at <= {now}";

/// Release policy shared by UI controls and stored watched mutations.
/// Specials are always eligible. Unknown seasons follow the regular-episode rule.
#[must_use]
pub fn can_mark_watched(
    kind: MediaKind,
    season: Option<u32>,
    released_at: Option<i64>,
    now: i64,
) -> bool {
    match kind {
        MediaKind::Series => true,
        MediaKind::Movie => released_at.is_none_or(|date| date <= now),
        MediaKind::Episode => season == Some(0) || released_at.is_some_and(|date| date <= now),
    }
}

fn summary_sql(now: &str, parent: &str) -> String {
    include_str!("watch_status.sql")
        .replace("{eligible}", ELIGIBLE)
        .replace("{retained}", RETAINED)
        .replace("{now}", now)
        .replace("{parent}", parent)
}

pub(crate) fn completion_sql(parent: &str, now: &str) -> String {
    // A badge needs no progress sums. Stop at the first incomplete regular episode.
    let retained = RETAINED.replace("{now}", now);
    format!(
        "EXISTS(SELECT 1 FROM media e JOIN episode_releases r USING(media_id) WHERE e.parent_id={parent} AND (r.season IS NULL OR r.season!=0)) AND NOT EXISTS(SELECT 1 FROM media e JOIN episode_releases r USING(media_id) LEFT JOIN watched w USING(media_id) WHERE e.parent_id={parent} AND (r.season IS NULL OR r.season!=0) AND NOT (({retained}) AND COALESCE(w.watched,0)=1))"
    )
}

pub(crate) fn install_view(connection: &mut SqliteConnection) -> Result<()> {
    connection.batch_execute(
        &include_str!("effective_watched.sql")
            .replace("{series_status}", &summary_sql("unixepoch()", "m.media_id")),
    )?;
    Ok(())
}

pub(crate) fn can_mark_stored_watched(connection: &mut SqliteConnection, id: i64) -> Result<bool> {
    #[derive(QueryableByName)]
    struct Release {
        #[diesel(sql_type = diesel::sql_types::Text)]
        kind: String,
        #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
        season: Option<i64>,
        #[diesel(sql_type = diesel::sql_types::Nullable<BigInt>)]
        released_at: Option<i64>,
        #[diesel(sql_type = BigInt)]
        now: i64,
    }
    let release = diesel::sql_query(
        "SELECT m.kind, r.season, r.released_at, unixepoch() AS now FROM media m LEFT JOIN episode_releases r ON r.media_id=m.media_id WHERE m.media_id=?",
    )
        .bind::<BigInt, _>(id)
        .get_result::<Release>(connection)?;
    Ok(can_mark_watched(
        MediaKind::parse(&release.kind)?,
        release.season.map(u32::try_from).transpose()?,
        release.released_at,
        release.now,
    ))
}

#[derive(Clone, Copy)]
pub(crate) enum ReleaseScope {
    Item(i64),
    Series(i64),
}

/// Newly learned future dates invalidate watches, except for season-zero specials.
/// Missing dates preserve existing watched flags and all playback history.
pub(crate) fn clear_future_watches(
    connection: &mut SqliteConnection,
    scope: ReleaseScope,
) -> Result<()> {
    let (column, id) = match scope {
        ReleaseScope::Item(id) => ("e.media_id", id),
        ReleaseScope::Series(id) => ("e.parent_id", id),
    };
    diesel::sql_query(format!(
        "UPDATE watched SET watched=0 WHERE watched=1 AND media_id IN (SELECT e.media_id FROM media e JOIN episode_releases r ON r.media_id=e.media_id WHERE {column}=? AND r.released_at>unixepoch() AND (e.kind!='episode' OR r.season IS NULL OR r.season!=0))"
    ))
    .bind::<BigInt, _>(id)
    .execute(connection)?;
    Ok(())
}

#[derive(QueryableByName)]
struct Summary {
    #[diesel(sql_type = BigInt)]
    total: i64,
    #[diesel(sql_type = BigInt)]
    released: i64,
    #[diesel(sql_type = BigInt)]
    watched: i64,
    #[diesel(sql_type = diesel::sql_types::Double)]
    completed: f64,
    #[diesel(sql_type = diesel::sql_types::Double)]
    resumed: f64,
    #[diesel(sql_type = diesel::sql_types::Bool)]
    complete: bool,
}

impl Summary {
    fn progress(&self) -> Result<SeriesProgress> {
        Ok(SeriesProgress {
            total_episodes: u32::try_from(self.total)?,
            released_episodes: u32::try_from(self.released)?,
            watched_episodes: u32::try_from(self.watched)?,
            completed_equivalents: self.completed,
            resume_equivalents: self.resumed,
        })
    }
}

impl Library {
    fn series_status(&self, parent: &MediaKey, now: i64) -> Result<Summary> {
        ensure!(
            parent.kind == MediaKind::Series,
            "invalid series progress key"
        );
        let connection = &mut *self.connection.borrow_mut();
        let parent_id = media_id(connection, parent)?;
        // One bound clock value is shared by all release checks in the query.
        let sql = format!(
            "WITH clock(now) AS (VALUES (?)) {}",
            summary_sql("(SELECT now FROM clock)", "?")
        );
        Ok(diesel::sql_query(sql)
            .bind::<BigInt, _>(now)
            .bind::<BigInt, _>(parent_id)
            .get_result(connection)?)
    }

    /// Aggregate regular episodes across the full inventory, excluding season zero.
    /// Existing undated watched flags count; known future episodes do not.
    /// Resume-based advancement is separate and does not establish watched status.
    /// # Errors
    /// Returns an error for invalid keys, a missing parent, or failed reads.
    pub fn series_progress(&self, parent: &MediaKey, now: i64) -> Result<SeriesProgress> {
        self.series_status(parent, now)?.progress()
    }

    /// Derive the badge and normalized progress from the shared series summary.
    /// # Errors
    /// Returns an error for invalid keys, a missing parent, or failed reads.
    pub fn series_poster_state(
        &self,
        parent: &MediaKey,
        now: i64,
    ) -> Result<(bool, Option<Progress>)> {
        let status = self.series_status(parent, now)?;
        let progress = status.progress()?.normalized_progress();
        Ok((status.complete, progress))
    }
}
