//! Add historical state without recording new playback or replacing local choices.
use anyhow::{Result, ensure};
use diesel::prelude::*;

use crate::{
    EpisodeRelease, Library, Media, MediaKind, Progress,
    catalog::media::{get_media, media_id, upsert_media, validate_media},
    playback::watch_status::can_mark_stored_watched,
    storage::schema::{episode_releases, media, progress, viewing, watched},
};

/// A movie or episode's historical flags and optional resume point.
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub media: Media,
    pub watched: bool,
    pub progress: Option<Progress>,
    pub active: bool,
}

/// A bounded import batch, with complete episode inventories when available.
#[derive(Clone, Debug, Default)]
pub struct HistoryImport {
    pub parents: Vec<Media>,
    pub inventory: Vec<EpisodeRelease>,
    pub entries: Vec<HistoryEntry>,
}

/// Counts applied fields and unchanged, preserved, or ineligible entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryImportResult {
    pub watched: usize,
    pub progress: usize,
    pub unchanged: usize,
    pub preserved: usize,
    pub ineligible: usize,
    pub notices: Vec<HistoryImportNotice>,
}

/// A per-item explanation for history that was not applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryImportNotice {
    pub title: String,
    pub reason: &'static str,
}

fn insert_release(connection: &mut SqliteConnection, release: &EpisodeRelease) -> Result<()> {
    ensure!(
        release.media.key.kind == MediaKind::Episode && release.media.parent.is_some(),
        "invalid history episode"
    );
    let id = insert_missing(connection, &release.media)?;
    diesel::insert_into(episode_releases::table)
        .values((
            episode_releases::media_id.eq(id),
            episode_releases::released_at.eq(release.released_at),
            episode_releases::season.eq(release.season.map(i64::from)),
            episode_releases::episode.eq(release.episode.map(i64::from)),
        ))
        .on_conflict(episode_releases::media_id)
        .do_nothing()
        .execute(connection)?;
    Ok(())
}

fn insert_missing(connection: &mut SqliteConnection, item: &Media) -> Result<i64> {
    validate_media(item)?;
    if let Some(existing) = get_media(connection, &item.key)? {
        ensure!(existing.parent == item.parent, "conflicting history parent");
        media_id(connection, &item.key)
    } else {
        upsert_media(connection, item)
    }
}

fn insert_progress(
    connection: &mut SqliteConnection,
    id: i64,
    entry: &HistoryEntry,
    sample: Progress,
    result: &mut HistoryImportResult,
) -> Result<()> {
    let mut active = entry.active;
    if active && let Some(parent) = &entry.media.parent {
        let parent_id = media_id(connection, parent)?;
        let has_local_resume = diesel::select(diesel::dsl::exists(
            viewing::table
                .inner_join(media::table)
                .filter(media::parent_id.eq(parent_id)),
        ))
        .get_result::<bool>(connection)?;
        if has_local_resume {
            active = false;
            result.preserved += 1;
            result.notices.push(HistoryImportNotice {
                title: entry.media.title.clone(),
                reason: "Resume point stored; existing Continue Watching choice or dismissal kept",
            });
        }
    }
    diesel::insert_into(progress::table)
        .values((
            progress::media_id.eq(id),
            progress::position_seconds.eq(sample.position_seconds),
            progress::duration_seconds.eq(sample.duration_seconds),
        ))
        .execute(connection)?;
    // Historical offsets are not measured watched time. A zero-credit row
    // makes resume available without generating a live playback session.
    diesel::insert_into(viewing::table)
        .values((
            viewing::media_id.eq(id),
            viewing::watched_seconds.eq(0.0),
            viewing::active.eq(active),
            viewing::touched.eq(0_i64),
        ))
        .execute(connection)?;
    result.progress += 1;
    Ok(())
}

impl Library {
    /// Merge historical watches and resume positions. Existing local state wins.
    /// No saved membership, playback statistics, or session events are created.
    /// # Errors
    /// Returns an error for an invalid batch or failed writes; the whole batch rolls back.
    pub fn import_history(&mut self, input: &HistoryImport) -> Result<HistoryImportResult> {
        ensure!(
            input.entries.len() + input.parents.len() + input.inventory.len() <= 50_000,
            "too many history records"
        );
        self.immediate_transaction(|connection| {
            for parent in &input.parents {
                ensure!(
                    parent.key.kind == MediaKind::Series && parent.parent.is_none(),
                    "invalid history parent"
                );
                insert_missing(connection, parent)?;
            }
            for release in &input.inventory {
                insert_release(connection, release)?;
            }
            let mut result = HistoryImportResult::default();
            for entry in &input.entries {
                ensure!(
                    entry.media.key.kind != MediaKind::Series,
                    "series history must identify episodes"
                );
                if let Some(sample) = entry.progress {
                    ensure!(
                        Progress::new(sample.position_seconds, sample.duration_seconds)
                            == Some(sample)
                            && sample.position_seconds > 0.0,
                        "invalid history progress"
                    );
                }
                let id = insert_missing(connection, &entry.media)?;
                let local_watched = watched::table
                    .find(id)
                    .select(watched::is_watched)
                    .first::<bool>(connection)
                    .optional()?;
                let local_progress = progress::table
                    .find(id)
                    .select((progress::position_seconds, progress::duration_seconds))
                    .first::<(f64, f64)>(connection)
                    .optional()?;
                let local_viewing = viewing::table
                    .find(id)
                    .select(viewing::active)
                    .first::<bool>(connection)
                    .optional()?;
                if local_watched.is_some() || local_progress.is_some() || local_viewing.is_some() {
                    let same_watched = !entry.watched || local_watched == Some(true);
                    let same_progress = entry.progress.is_none_or(|p| {
                        local_progress == Some((p.position_seconds, p.duration_seconds))
                    });
                    if same_watched && same_progress {
                        result.unchanged += 1;
                    } else {
                        result.preserved += 1;
                        result.notices.push(HistoryImportNotice { title: entry.media.title.clone(), reason: "Existing Scry state kept instead of imported history" });
                    }
                    continue;
                }
                if entry.watched {
                    if can_mark_stored_watched(connection, id)? {
                        diesel::insert_into(watched::table)
                            .values((watched::media_id.eq(id), watched::is_watched.eq(true)))
                            .execute(connection)?;
                        result.watched += 1;
                    } else {
                        result.ineligible += 1;
                        result.notices.push(HistoryImportNotice { title: entry.media.title.clone(), reason: "Watched state skipped: release date is unknown or in the future" });
                    }
                }
                if let Some(sample) = entry.progress {
                    insert_progress(connection, id, entry, sample, &mut result)?;
                }
            }
            Ok(result)
        })
    }
}
