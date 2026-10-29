use crate::{
    EpisodeRelease, Library, Media, MediaKey, MediaKind, Page, Progress,
    media::{MediaProgressRecord, hydrate_progress, media_id, upsert_media},
    schema::{episode_releases, media, series_inventory, watched},
    watch_status::{ReleaseScope, clear_future_watches},
};
use anyhow::{Context, Result, ensure};
use diesel::{prelude::*, upsert::excluded};
use std::collections::HashMap;

type Inventory<'a> = HashMap<(&'a str, &'a str), (usize, &'a EpisodeRelease)>;
// Leave room below SQLite's parameter limit when deleting a large inventory.
const DELETE_BATCH: usize = 500;

fn inventory(releases: &[EpisodeRelease]) -> Inventory<'_> {
    releases
        .iter()
        .enumerate()
        .map(|(index, release)| {
            (
                (
                    release.media.key.namespace.as_str(),
                    release.media.key.id.as_str(),
                ),
                (index, release),
            )
        })
        .collect()
}

impl Library {
    /// Keep the provider's full episode inventory without changing resume state.
    /// # Errors
    /// Returns an error for invalid episodes, missing parents, or failed writes. Changes are atomic.
    pub fn sync_episode_releases(
        &mut self,
        parent: &MediaKey,
        releases: &[EpisodeRelease],
    ) -> Result<bool> {
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| sync_episode_releases(connection, parent, releases))
    }

    /// Next known release time. Used to wake the UI when stored state changes with time.
    /// # Errors
    /// Returns an error on database reads.
    pub fn next_release_at(&self, now: i64) -> Result<Option<i64>> {
        Ok(episode_releases::table
            .filter(episode_releases::released_at.gt(now))
            .select(diesel::dsl::min(episode_releases::released_at))
            .first::<Option<i64>>(&mut *self.connection.borrow_mut())?)
    }

    /// Unwatched saved movies and one next released, unwatched episode per saved show.
    /// # Errors
    /// Returns an error on database reads.
    pub fn list_up_next(&self, now: i64, page: Page) -> Result<Vec<(Media, Option<Progress>)>> {
        let connection = &mut *self.connection.borrow_mut();
        let records = diesel::sql_query("SELECT m.*, p.position_seconds, p.duration_seconds FROM watchlist w JOIN media m ON m.media_id=COALESCE((SELECT e.media_id FROM media e JOIN episode_releases r ON r.media_id=e.media_id WHERE e.parent_id=w.media_id AND r.released_at<=? AND NOT EXISTS(SELECT 1 FROM watched v WHERE v.media_id=e.media_id AND v.watched=1) ORDER BY CASE WHEN r.season IS NULL THEN 2 WHEN r.season=0 THEN 1 ELSE 0 END, r.season, r.episode, e.media_id LIMIT 1), w.media_id) LEFT JOIN progress p ON p.media_id=m.media_id WHERE m.kind IN ('movie','episode') AND NOT EXISTS(SELECT 1 FROM episode_releases r WHERE r.media_id=m.media_id AND r.released_at>?) AND NOT EXISTS(SELECT 1 FROM watched v WHERE v.media_id=m.media_id AND v.watched=1) ORDER BY w.entry_id DESC LIMIT ? OFFSET ?")
            .bind::<diesel::sql_types::BigInt, _>(now)
            .bind::<diesel::sql_types::BigInt, _>(now)
            .bind::<diesel::sql_types::BigInt, _>(i64::from(page.limit))
            .bind::<diesel::sql_types::BigInt, _>(i64::from(page.offset))
            .load::<MediaProgressRecord>(connection)?;
        hydrate_progress(connection, records)
    }
}

pub(crate) fn sync_episode_releases(
    connection: &mut SqliteConnection,
    parent: &MediaKey,
    releases: &[EpisodeRelease],
) -> Result<bool> {
    ensure!(
        parent.kind == MediaKind::Series
            && releases.len() <= 50_000
            && releases
                .iter()
                .all(|release| release.media.key.kind == MediaKind::Episode
                    && release.media.parent.as_ref() == Some(parent)),
        "invalid release inventory"
    );
    let incoming = inventory(releases);
    ensure!(
        incoming.len() == releases.len(),
        "duplicate episode releases"
    );
    let id = media_id(connection, parent)?;
    // A show action is represented by its episodes, never a sticky parent override.
    let cleared = diesel::delete(watched::table.find(id)).execute(connection)? > 0;
    let known = diesel::insert_into(series_inventory::table)
        .values(series_inventory::media_id.eq(id))
        .on_conflict_do_nothing()
        .execute(connection)?
        == 0;
    let mut removed = Vec::new();
    let mut unchanged = vec![false; releases.len()];
    let stored = media::table
        .inner_join(episode_releases::table)
        .filter(media::parent_id.eq(id))
        .select((
            media::media_id,
            media::namespace,
            media::external_id,
            media::title,
            episode_releases::released_at,
            episode_releases::season,
            episode_releases::episode,
        ))
        .load_iter::<(
            i64,
            String,
            String,
            String,
            Option<i64>,
            Option<i64>,
            Option<i64>,
        ), diesel::connection::DefaultLoadingMode>(connection)?;
    for record in stored {
        let (episode_id, namespace, external_id, title, date, season, episode) = record?;
        let key = (namespace.as_str(), external_id.as_str());
        match incoming.get(&key) {
            Some((index, release))
                if release.media.title == title
                    && release.released_at == date
                    && release.season.map(i64::from) == season
                    && release.episode.map(i64::from) == episode =>
            {
                *unchanged
                    .get_mut(*index)
                    .context("missing inventory entry")? = true;
            }
            Some(_) => {}
            None => {
                removed.push(episode_id);
            }
        }
    }
    if known && unchanged.iter().all(|value| *value) && removed.is_empty() {
        return Ok(cleared);
    }
    for ids in removed.chunks(DELETE_BATCH) {
        diesel::delete(episode_releases::table.filter(episode_releases::media_id.eq_any(ids)))
            .execute(connection)?;
    }
    // Keep provider order when assigning new media IDs.
    for (release, unchanged) in releases.iter().zip(unchanged) {
        if unchanged {
            continue;
        }
        upsert_media(connection, &release.media)?;
        let episode_id = media_id(connection, &release.media.key)?;
        diesel::insert_into(episode_releases::table)
            .values((
                episode_releases::media_id.eq(episode_id),
                episode_releases::released_at.eq(release.released_at),
                episode_releases::season.eq(release.season.map(i64::from)),
                episode_releases::episode.eq(release.episode.map(i64::from)),
            ))
            .on_conflict(episode_releases::media_id)
            .do_update()
            .set((
                episode_releases::released_at.eq(excluded(episode_releases::released_at)),
                episode_releases::season.eq(excluded(episode_releases::season)),
                episode_releases::episode.eq(excluded(episode_releases::episode)),
            ))
            .execute(connection)?;
    }
    clear_future_watches(connection, ReleaseScope::Series(id))?;
    Ok(true)
}
