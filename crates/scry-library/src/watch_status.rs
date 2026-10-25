//! Single owner of release eligibility, series counts, progress, and completion.
//! The database view uses the same summary query as explicit-time status reads.
use anyhow::{Result, ensure};
use diesel::{connection::SimpleConnection, prelude::*, sql_types::BigInt};

use crate::{Library, MediaKey, MediaKind, Progress, SeriesProgress, media::media_id};

const ELIGIBLE: &str = "r.released_at IS NULL OR r.released_at <= {now}";

fn summary_sql(now: &str, parent: &str) -> String {
    include_str!("watch_status.sql")
        .replace("{eligible}", ELIGIBLE)
        .replace("{now}", now)
        .replace("{parent}", parent)
}

pub(crate) fn install_view(connection: &mut SqliteConnection) -> Result<()> {
    connection.batch_execute(
        &include_str!("effective_watched.sql")
            .replace("{series_status}", &summary_sql("unixepoch()", "m.media_id")),
    )?;
    Ok(())
}

pub(crate) fn can_mark_watched(connection: &mut SqliteConnection, id: i64) -> Result<bool> {
    #[derive(QueryableByName)]
    struct Eligibility {
        #[diesel(sql_type = diesel::sql_types::Bool)]
        allowed: bool,
    }
    let sql = format!(
        "SELECT ({}) AS allowed FROM media m LEFT JOIN episode_releases r ON r.media_id=m.media_id WHERE m.media_id=?",
        ELIGIBLE.replace("{now}", "unixepoch()")
    );
    Ok(diesel::sql_query(sql)
        .bind::<BigInt, _>(id)
        .get_result::<Eligibility>(connection)?
        .allowed)
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

    /// Aggregate the full known inventory, not the visible episode page.
    /// Undated watched episodes count; known future episodes do not.
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
