use crate::{
    EpisodeRelease, Library, Media, MediaKey, MediaKind, Page, Progress,
    media::{MediaRecord, hydrate, media_id, upsert_media},
    schema::{episode_releases, media},
};
use anyhow::{Result, ensure};
use diesel::{prelude::*, upsert::excluded};
use std::collections::HashMap;

impl Library {
    /// Keep the provider's dated episode inventory without changing watched or resume state.
    /// # Errors
    /// Returns an error for invalid episodes, missing parents, or failed writes. Changes are atomic.
    pub fn sync_episode_releases(
        &mut self,
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
        let incoming: HashMap<_, _> = releases
            .iter()
            .map(|release| {
                (
                    (
                        release.media.key.namespace.clone(),
                        release.media.key.id.clone(),
                    ),
                    (
                        release.media.title.clone(),
                        release.released_at,
                        release.season.map(i64::from),
                        release.episode.map(i64::from),
                    ),
                )
            })
            .collect();
        ensure!(
            incoming.len() == releases.len(),
            "duplicate episode releases"
        );
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                let id = media_id(connection, parent)?;
                let stored = media::table
                    .inner_join(episode_releases::table)
                    .filter(media::parent_id.eq(id))
                    .select((
                        media::namespace,
                        media::external_id,
                        media::title,
                        episode_releases::released_at,
                        episode_releases::season,
                        episode_releases::episode,
                    ))
                    .load::<(String, String, String, i64, Option<i64>, Option<i64>)>(connection)?;
                let existing: HashMap<_, _> = stored
                    .into_iter()
                    .map(|(namespace, external_id, title, date, season, episode)| {
                        ((namespace, external_id), (title, date, season, episode))
                    })
                    .collect();
                if existing == incoming {
                    return Ok(false);
                }
                diesel::delete(
                    episode_releases::table.filter(
                        episode_releases::media_id.eq_any(
                            media::table
                                .filter(media::parent_id.eq(id))
                                .select(media::media_id),
                        ),
                    ),
                )
                .execute(connection)?;
                for release in releases {
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
                            episode_releases::released_at
                                .eq(excluded(episode_releases::released_at)),
                            episode_releases::season.eq(excluded(episode_releases::season)),
                            episode_releases::episode.eq(excluded(episode_releases::episode)),
                        ))
                        .execute(connection)?;
                }
                Ok(true)
            })
    }

    /// Unwatched saved movies and known released episodes of saved shows, including partial playback.
    /// Filters apply before pagination. Explicitly watched shows hide their episodes.
    /// # Errors
    /// Returns an error on database reads.
    pub fn list_up_next(&self, now: i64, page: Page) -> Result<Vec<(Media, Option<Progress>)>> {
        let items = {
            let connection = &mut *self.connection.borrow_mut();
            let records = diesel::sql_query("SELECT m.* FROM media m JOIN watchlist w ON w.media_id=COALESCE(m.parent_id,m.media_id) LEFT JOIN episode_releases r ON r.media_id=m.media_id WHERE (m.kind='movie' OR (m.kind='episode' AND r.released_at<=?)) AND NOT EXISTS(SELECT 1 FROM watched v WHERE v.media_id=m.media_id AND v.watched=1) AND (m.parent_id IS NULL OR NOT EXISTS(SELECT 1 FROM watched v WHERE v.media_id=m.parent_id AND v.watched=1)) ORDER BY w.entry_id DESC, CASE WHEN r.season IS NULL THEN 2 WHEN r.season=0 THEN 1 ELSE 0 END, r.season, r.episode, m.media_id LIMIT ? OFFSET ?")
                .bind::<diesel::sql_types::BigInt, _>(now)
                .bind::<diesel::sql_types::BigInt, _>(i64::from(page.limit))
                .bind::<diesel::sql_types::BigInt, _>(i64::from(page.offset))
                .load::<MediaRecord>(connection)?;
            hydrate(connection, records)?
        };
        items
            .into_iter()
            .map(|item| {
                let progress = self.load_progress(&item.key)?;
                Ok((item, progress))
            })
            .collect()
    }
}
