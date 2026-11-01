use anyhow::{Context, Result, ensure};
use diesel::{dsl::max, prelude::*, upsert::excluded};

use crate::{
    COMPLETION_THRESHOLD, DetailSnapshot, Library, Media, MediaKey, MediaKind, MediaState, Page,
    Progress, SelectedState, WATCHED_THRESHOLD, WatchedState,
    catalog::media::{
        MediaProgressRecord, get_media, hydrate_progress, media_id, upsert_media,
        upsert_media_with_parent,
    },
    playback::{
        up_next,
        watch_status::{ReleaseScope, can_mark_stored_watched, clear_future_watches},
    },
    stats::metrics as stats,
    storage::schema::{effective_watched, episode_releases, media, progress, viewing, watched},
};

fn store_release(connection: &mut SqliteConnection, id: i64, date: Option<i64>) -> Result<()> {
    if let Some(date) = date {
        diesel::insert_into(episode_releases::table)
            .values((
                episode_releases::media_id.eq(id),
                episode_releases::released_at.eq(date),
            ))
            .on_conflict(episode_releases::media_id)
            .do_update()
            .set(episode_releases::released_at.eq(date))
            .execute(connection)?;
        clear_future_watches(connection, ReleaseScope::Item(id))?;
    }
    Ok(())
}

fn watched_state(connection: &mut SqliteConnection, id: i64) -> Result<WatchedState> {
    let state = effective_watched::table
        .find(id)
        .select(effective_watched::is_watched)
        .first::<bool>(connection)
        .optional()?;
    Ok(WatchedState {
        watched: state.unwrap_or(false),
    })
}

fn load_progress_by_id(connection: &mut SqliteConnection, id: i64) -> Result<Option<Progress>> {
    Ok(progress::table
        .find(id)
        .select((progress::position_seconds, progress::duration_seconds))
        .first::<(f64, f64)>(connection)
        .optional()?
        .map(|(position_seconds, duration_seconds)| Progress {
            position_seconds,
            duration_seconds,
        }))
}

fn media_states_in(
    connection: &mut SqliteConnection,
    keys: &[MediaKey],
) -> Result<Vec<MediaState>> {
    #[derive(QueryableByName)]
    struct Record {
        #[diesel(sql_type = diesel::sql_types::Bool)]
        stored: bool,
        #[diesel(sql_type = diesel::sql_types::Bool)]
        watched: bool,
        #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Double>)]
        position_seconds: Option<f64>,
        #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Double>)]
        duration_seconds: Option<f64>,
        #[diesel(sql_type = diesel::sql_types::Double)]
        watched_seconds: f64,
    }
    ensure!(
        keys.len() <= crate::MAX_PAGE_SIZE as usize,
        "too many media state keys"
    );
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    // A LEFT JOIN to the derived view can materialize the entire library.
    // A keyed scalar read keeps this query proportional to the requested batch.
    let records = diesel::sql_query("SELECT m.media_id IS NOT NULL AS stored, COALESCE((SELECT ew.watched FROM effective_watched ew WHERE ew.media_id=m.media_id),0) AS watched, p.position_seconds, p.duration_seconds, COALESCE(v.watched_seconds,0.0) AS watched_seconds FROM json_each(?) k LEFT JOIN media m ON m.namespace=json_extract(k.value,'$.namespace') AND m.kind=json_extract(k.value,'$.kind') AND m.external_id=json_extract(k.value,'$.id') LEFT JOIN progress p ON p.media_id=m.media_id LEFT JOIN viewing v ON v.media_id=m.media_id ORDER BY CAST(k.key AS INTEGER)")
        .bind::<diesel::sql_types::Text, _>(serde_json::to_string(keys)?)
        .load::<Record>(connection)?;
    Ok(keys
        .iter()
        .cloned()
        .zip(records)
        .map(|(key, record)| MediaState {
            key,
            stored: record.stored,
            watched_seconds: record.watched_seconds,
            watched: WatchedState {
                watched: record.watched,
            },
            progress: record.position_seconds.zip(record.duration_seconds).map(
                |(position_seconds, duration_seconds)| Progress {
                    position_seconds,
                    duration_seconds,
                },
            ),
        })
        .collect())
}

fn set_watched(connection: &mut SqliteConnection, id: i64, value: bool) -> Result<()> {
    let kind = media::table
        .find(id)
        .select(media::kind)
        .first::<String>(connection)?;
    if kind == "series" {
        let children = media::table
            .filter(media::parent_id.eq(id))
            .select(media::media_id)
            .load::<i64>(connection)?;
        diesel::delete(watched::table.find(id)).execute(connection)?;
        for child in children {
            set_watched(connection, child, value)?;
        }
        return Ok(());
    }
    if value && !can_mark_stored_watched(connection, id)? {
        tracing::debug!("Skipped marking an unreleased item watched");
        return Ok(());
    }
    let was_watched = watched_state(connection, id)?.watched;
    stats::watched_at(connection, id, value, was_watched)?;
    diesel::insert_into(watched::table)
        .values((watched::media_id.eq(id), watched::is_watched.eq(value)))
        .on_conflict(watched::media_id)
        .do_update()
        .set(watched::is_watched.eq(value))
        .execute(connection)?;
    diesel::delete(progress::table.find(id)).execute(connection)?;
    diesel::delete(viewing::table.find(id)).execute(connection)?;
    stats::finish_run(connection, id)?;
    Ok(())
}

fn update_watched(connection: &mut SqliteConnection, id: i64, automatic: bool) -> Result<()> {
    let state = watched_state(connection, id)?;
    let value = state.watched || (automatic && can_mark_stored_watched(connection, id)?);
    stats::watched_at(connection, id, value, state.watched)?;
    diesel::insert_into(watched::table)
        .values((watched::media_id.eq(id), watched::is_watched.eq(value)))
        .on_conflict(watched::media_id)
        .do_update()
        .set(watched::is_watched.eq(value))
        .execute(connection)?;
    Ok(())
}

fn save_progress(
    connection: &mut SqliteConnection,
    id: i64,
    position: f64,
    duration: f64,
) -> Result<()> {
    diesel::insert_into(progress::table)
        .values((
            progress::media_id.eq(id),
            progress::position_seconds.eq(position),
            progress::duration_seconds.eq(duration),
        ))
        .on_conflict(progress::media_id)
        .do_update()
        .set((
            progress::position_seconds.eq(excluded(progress::position_seconds)),
            progress::duration_seconds.eq(excluded(progress::duration_seconds)),
        ))
        .execute(connection)?;
    Ok(())
}

fn activate(connection: &mut SqliteConnection, id: i64, delta: f64) -> Result<f64> {
    let previous = viewing::table
        .find(id)
        .select(viewing::watched_seconds)
        .first::<f64>(connection)
        .optional()?
        .unwrap_or(0.0);
    let total = previous + delta;
    ensure!(total.is_finite(), "watched time overflow");
    let last: Option<i64> = viewing::table
        .select(max(viewing::touched))
        .first(connection)?;
    let touched = last
        .unwrap_or(0)
        .checked_add(1)
        .context("viewing order overflow")?;
    diesel::insert_into(viewing::table)
        .values((
            viewing::media_id.eq(id),
            viewing::watched_seconds.eq(total),
            viewing::active.eq(true),
            viewing::touched.eq(touched),
        ))
        .on_conflict(viewing::media_id)
        .do_update()
        .set((
            viewing::watched_seconds.eq(total),
            viewing::active.eq(true),
            viewing::touched.eq(touched),
        ))
        .execute(connection)?;
    Ok(total)
}

fn validate_sample(
    kind: MediaKind,
    position: f64,
    duration: f64,
    watched_delta: f64,
) -> Result<Progress> {
    ensure!(
        kind != MediaKind::Series && watched_delta.is_finite() && watched_delta >= 0.0,
        "invalid playback sample"
    );
    Progress::new(position, duration).context("invalid playback sample")
}

fn record_sample_by_id(
    connection: &mut SqliteConnection,
    id: i64,
    sample: Progress,
    watched_delta: f64,
) -> Result<()> {
    let duration = sample.duration_seconds;
    stats::runtime(connection, id, duration)?;
    save_progress(
        connection,
        id,
        sample.position_seconds,
        sample.duration_seconds,
    )?;
    let parent = media::table
        .find(id)
        .select(media::parent_id)
        .first::<Option<i64>>(connection)?;
    if let Some(parent) = parent {
        let siblings = media::table
            .filter(media::parent_id.eq(parent))
            .filter(media::media_id.ne(id))
            .select(media::media_id);
        diesel::update(
            viewing::table
                .filter(viewing::media_id.eq_any(siblings))
                .filter(viewing::active.eq(true)),
        )
        .set(viewing::active.eq(false))
        .execute(connection)?;
    }
    let total = activate(connection, id, watched_delta)?;
    stats::record(connection, id, watched_delta, duration)?;
    update_watched(connection, id, total > duration * WATCHED_THRESHOLD)
}

impl Library {
    /// Register release metadata without changing watched or playback state.
    /// Missing dates preserve the last known date.
    /// # Errors
    /// Returns an error for invalid metadata or failed writes.
    pub fn register_release(&mut self, item: &Media, date: Option<i64>) -> Result<()> {
        self.immediate_transaction(|connection| {
            let id = upsert_media(connection, item)?;
            store_release(connection, id, date)
        })
    }

    /// Register a captured metadata snapshot and apply a watched action atomically.
    /// Regular episodes need a known, passed release date. Specials allow any date.
    /// Ineligible watched actions are skipped, not errors. Movie rules are unchanged.
    /// # Errors
    /// Returns an error for invalid metadata/inventories or failed writes.
    pub fn set_media_watched(
        &mut self,
        item: &Media,
        date: Option<i64>,
        inventory: Option<&[crate::EpisodeRelease]>,
        value: bool,
    ) -> Result<()> {
        self.immediate_transaction(|connection| {
            let id = upsert_media(connection, item)?;
            if let Some(inventory) = inventory {
                up_next::sync_episode_releases(connection, &item.key, inventory)?;
            }
            if item.key.kind != MediaKind::Series {
                store_release(connection, id, date)?;
            }
            set_watched(connection, id, value)
        })
    }
    /// Read a bounded set of identities in one query, preserving order and missing entries.
    /// # Errors
    /// Returns an error for an oversized request or failed reads.
    pub fn media_states(&self, keys: &[MediaKey]) -> Result<Vec<MediaState>> {
        self.with_connection(|connection| media_states_in(connection, keys))
    }

    /// Read the selected media state and up to one page of episode watched flags atomically.
    /// # Errors
    /// Returns an error for an oversized episode list or failed reads.
    pub fn detail_snapshot(
        &self,
        selected: Option<&MediaKey>,
        episodes: &[MediaKey],
    ) -> Result<DetailSnapshot> {
        ensure!(
            episodes.len() <= crate::MAX_PAGE_SIZE as usize,
            "too many visible episodes"
        );
        self.transaction(|connection| {
            let record = match selected {
                Some(key) => media::table
                    .filter(media::namespace.eq(&key.namespace))
                    .filter(media::kind.eq(key.kind.as_str()))
                    .filter(media::external_id.eq(&key.id))
                    .select((media::media_id, media::parent_id))
                    .first::<(i64, Option<i64>)>(connection)
                    .optional()?
                    .map(|(id, parent_id)| (key, id, parent_id)),
                None => None,
            };
            let series_id = record.and_then(|(key, id, parent_id)| match key.kind {
                MediaKind::Series => Some(id),
                MediaKind::Episode => parent_id,
                MediaKind::Movie => None,
            });
            let episode_states = if !episodes.is_empty() {
                media_states_in(connection, episodes)?
                    .into_iter()
                    .map(|state| (state.key, state.watched))
                    .collect()
            } else if let Some(series_id) = series_id {
                media::table
                    .left_join(watched::table)
                    .filter(media::parent_id.eq(series_id))
                    .order(media::media_id)
                    .limit(i64::from(crate::MAX_PAGE_SIZE))
                    .select((
                        media::namespace,
                        media::kind,
                        media::external_id,
                        watched::is_watched.nullable(),
                    ))
                    .load::<(String, String, String, Option<bool>)>(connection)?
                    .into_iter()
                    .map(|(namespace, kind, external_id, is_watched)| {
                        Ok((
                            MediaKey::new(namespace, MediaKind::parse(&kind)?, external_id),
                            WatchedState {
                                watched: is_watched.unwrap_or(false),
                            },
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?
            } else {
                Vec::new()
            };
            let selected_state = match record {
                Some((key, id, parent_id)) => Some(SelectedState {
                    key: key.clone(),
                    watched: watched_state(connection, id)?,
                    watchlist: crate::collections::watchlist::contains(
                        connection,
                        parent_id.unwrap_or(id),
                    )?,
                    progress: load_progress_by_id(connection, id)?,
                }),
                None => None,
            };
            Ok(DetailSnapshot {
                selected_state,
                episode_states,
            })
        })
    }

    /// # Errors
    /// Returns an error for missing media or failed reads.
    pub fn watched_state(&self, key: &MediaKey) -> Result<WatchedState> {
        self.with_connection(|connection| {
            let id = media_id(connection, key)?;
            watched_state(connection, id)
        })
    }

    /// # Errors
    /// Returns an error for missing media or failed writes.
    pub fn set_watched(&mut self, key: &MediaKey, value: bool) -> Result<()> {
        self.immediate_transaction(|connection| {
            let id = media_id(connection, key)?;
            set_watched(connection, id, value)
        })
    }

    /// Mark a captured set of episodes in one transaction, including episodes not yet registered.
    /// # Errors
    /// Returns an error for invalid media, a wrong parent, or failed writes. Nothing is partially saved.
    pub fn set_episodes_watched(
        &mut self,
        parent: &Media,
        episodes: &[Media],
        value: bool,
    ) -> Result<()> {
        self.set_episodes_watched_with_inventory(parent, episodes, None, value)
    }

    /// Apply a season action against its captured release inventory in one transaction.
    /// # Errors
    /// Returns an error for invalid metadata/inventory or failed writes.
    pub fn set_episodes_watched_with_inventory(
        &mut self,
        parent: &Media,
        episodes: &[Media],
        inventory: Option<&[crate::EpisodeRelease]>,
        value: bool,
    ) -> Result<()> {
        ensure!(
            parent.key.kind == MediaKind::Series
                && episodes.len() <= 50_000
                && episodes
                    .iter()
                    .all(|item| item.key.kind == MediaKind::Episode
                        && item.parent.as_ref() == Some(&parent.key)),
            "invalid episode batch"
        );
        self.immediate_transaction(|connection| {
            let parent_id = upsert_media(connection, parent)?;
            if let Some(inventory) = inventory {
                up_next::sync_episode_releases(connection, &parent.key, inventory)?;
            }
            for item in episodes {
                let id = upsert_media_with_parent(connection, item, Some(parent_id))?;
                set_watched(connection, id, value)?;
            }
            Ok(())
        })
    }

    /// Save resume position only. Seeking must never mark an item watched.
    /// # Errors
    /// Returns an error for invalid progress, missing media, or failed writes.
    pub fn save_progress(
        &mut self,
        key: &MediaKey,
        position_seconds: f64,
        duration_seconds: f64,
    ) -> Result<()> {
        ensure!(
            key.kind != MediaKind::Series,
            "progress must be finite with a positive duration"
        );
        let sample = Progress::new(position_seconds, duration_seconds)
            .context("progress must be finite with a positive duration")?;
        self.immediate_transaction(|connection| {
            let id = media_id(connection, key)?;
            stats::runtime(connection, id, sample.duration_seconds)?;
            save_progress(
                connection,
                id,
                sample.position_seconds,
                sample.duration_seconds,
            )
        })
    }

    /// # Errors
    /// Returns an error for missing media or failed reads.
    pub fn load_progress(&self, key: &MediaKey) -> Result<Option<Progress>> {
        self.with_connection(|connection| {
            let id = media_id(connection, key)?;
            load_progress_by_id(connection, id)
        })
    }

    /// Store a playback sample and actual playback time, excluding seeks.
    /// # Errors
    /// Returns an error for invalid values, missing media, or failed writes.
    pub fn record_playback(
        &mut self,
        key: &MediaKey,
        position: f64,
        duration: f64,
        watched_delta: f64,
    ) -> Result<()> {
        let sample = validate_sample(key.kind, position, duration, watched_delta)?;
        self.immediate_transaction(|connection| {
            let id = media_id(connection, key)?;
            record_sample_by_id(connection, id, sample, watched_delta)
        })
    }

    /// Register metadata and save its sample atomically. Parents must already exist.
    /// # Errors
    /// Returns an error for invalid metadata/samples, missing parents, or failed writes.
    pub fn record_media_playback(
        &mut self,
        item: &Media,
        position: f64,
        duration: f64,
        watched_delta: f64,
    ) -> Result<()> {
        let sample = validate_sample(item.key.kind, position, duration, watched_delta)?;
        self.immediate_transaction(|connection| {
            let id = upsert_media(connection, item)?;
            record_sample_by_id(connection, id, sample, watched_delta)
        })
    }

    /// Reconcile resume state when playback stops beyond the credits threshold.
    /// The caller supplies only a known released next episode.
    /// # Errors
    /// Returns an error for missing media, invalid next identity, or failed writes.
    pub fn finish_playback(&mut self, key: &MediaKey, next: Option<&Media>) -> Result<()> {
        self.immediate_transaction(|connection| {
            let id = media_id(connection, key)?;
            let Some(sample) = load_progress_by_id(connection, id)? else {
                return Ok(());
            };
            if sample.position_seconds <= sample.duration_seconds * COMPLETION_THRESHOLD {
                stats::finish_run(connection, id)?;
                return Ok(());
            }
            if let Some(next) = next {
                let current = get_media(connection, key)?.context("missing current media")?;
                ensure!(
                    key.kind == MediaKind::Episode
                        && next.key.kind == MediaKind::Episode
                        && current.parent.is_some()
                        && next.parent == current.parent
                        && next.key != *key,
                    "invalid next episode"
                );
            }
            diesel::update(viewing::table.find(id))
                .set(viewing::active.eq(false))
                .execute(connection)?;
            stats::finish_run(connection, id)?;
            diesel::update(progress::table.find(id))
                .set(progress::position_seconds.eq(0.0))
                .execute(connection)?;
            if let Some(next) = next {
                let next_id = upsert_media(connection, next)?;
                if watched_state(connection, next_id)?.watched {
                    return Ok(());
                }
                diesel::insert_into(progress::table)
                    .values((
                        progress::media_id.eq(next_id),
                        progress::position_seconds.eq(0.0),
                        progress::duration_seconds.eq(1.0),
                    ))
                    .on_conflict(progress::media_id)
                    .do_nothing()
                    .execute(connection)?;
                activate(connection, next_id, 0.0)?;
            }
            Ok(())
        })
    }

    /// Hide a movie or a show's episodes from Currently watching without
    /// changing library membership, resume positions, or watched history.
    /// New playback automatically makes the selected video active again.
    /// # Errors
    /// Returns an error for missing media or failed writes.
    pub fn dismiss_currently_watching(&mut self, key: &MediaKey) -> Result<()> {
        self.immediate_transaction(|connection| {
            let id = media_id(connection, key)?;
            let parent = if key.kind == MediaKind::Series {
                Some(id)
            } else {
                media::table
                    .find(id)
                    .select(media::parent_id)
                    .first::<Option<i64>>(connection)?
            };
            if let Some(parent) = parent {
                let episodes = media::table
                    .filter(media::parent_id.eq(parent))
                    .select(media::media_id);
                diesel::update(viewing::table.filter(viewing::media_id.eq_any(episodes)))
                    .set(viewing::active.eq(false))
                    .execute(connection)?;
            } else {
                diesel::update(viewing::table.find(id))
                    .set(viewing::active.eq(false))
                    .execute(connection)?;
            }
            Ok(())
        })
    }

    /// # Errors
    /// Returns an error on database reads. One most-recent entry per series.
    pub fn list_currently_watching(&self, page: Page) -> Result<Vec<(Media, Progress)>> {
        self.with_connection(|connection| {
            let records = diesel::sql_query("SELECT m.*, p.position_seconds, p.duration_seconds FROM media m JOIN viewing v ON v.media_id=m.media_id JOIN progress p ON p.media_id=m.media_id WHERE v.active=1 AND NOT EXISTS(SELECT 1 FROM viewing newer JOIN media nm ON nm.media_id=newer.media_id WHERE newer.active=1 AND m.parent_id IS NOT NULL AND nm.parent_id=m.parent_id AND newer.touched>v.touched) ORDER BY v.touched DESC LIMIT ? OFFSET ?")
                .bind::<diesel::sql_types::BigInt, _>(i64::from(page.limit)).bind::<diesel::sql_types::BigInt, _>(i64::from(page.offset)).load::<MediaProgressRecord>(connection)?;
            let items = hydrate_progress(connection, records)?;
            items
                .into_iter()
                .map(|(item, progress)| {
                    let sample = progress.context("missing viewing progress")?;
                    Ok((item, sample))
                })
                .collect()
        })
    }
}
