use anyhow::{Context, Result, ensure};
use diesel::{dsl::max, prelude::*, upsert::excluded};

use crate::{
    COMPLETION_THRESHOLD, Library, Media, MediaKey, MediaKind, Page, Progress, WATCHED_THRESHOLD,
    WatchedState, is_complete,
    media::{MediaRecord, hydrate, media_id, upsert_media},
    schema::{media, progress, viewing, watched},
};

fn watched_state(connection: &mut SqliteConnection, id: i64) -> Result<WatchedState> {
    let state = watched::table
        .find(id)
        .select((watched::is_watched, watched::manual_override))
        .first::<(bool, Option<bool>)>(connection)
        .optional()?;
    Ok(state.map_or(
        WatchedState {
            watched: false,
            manual_override: None,
        },
        |(watched, manual_override)| WatchedState {
            watched,
            manual_override,
        },
    ))
}

fn update_watched(connection: &mut SqliteConnection, id: i64, automatic: bool) -> Result<()> {
    let state = watched_state(connection, id)?;
    let value = state.manual_override.unwrap_or(state.watched || automatic);
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

impl Library {
    /// # Errors
    /// Returns an error for missing media or failed reads.
    pub fn watched_state(&self, key: &MediaKey) -> Result<WatchedState> {
        let connection = &mut *self.connection.borrow_mut();
        let id = media_id(connection, key)?;
        watched_state(connection, id)
    }

    /// # Errors
    /// Returns an error for missing media or failed writes.
    pub fn set_watched(&mut self, key: &MediaKey, value: bool) -> Result<()> {
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                let id = media_id(connection, key)?;
                diesel::insert_into(watched::table)
                    .values((
                        watched::media_id.eq(id),
                        watched::is_watched.eq(value),
                        watched::manual_override.eq(Some(value)),
                    ))
                    .on_conflict(watched::media_id)
                    .do_update()
                    .set((
                        watched::is_watched.eq(value),
                        watched::manual_override.eq(Some(value)),
                    ))
                    .execute(connection)?;
                diesel::delete(progress::table.find(id)).execute(connection)?;
                diesel::delete(viewing::table.find(id)).execute(connection)?;
                Ok(())
            })
    }

    /// # Errors
    /// Returns an error for missing media or failed writes.
    pub fn clear_watched_override(&mut self, key: &MediaKey) -> Result<()> {
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                let id = media_id(connection, key)?;
                let sample = viewing::table
                    .inner_join(progress::table.on(progress::media_id.eq(viewing::media_id)))
                    .filter(viewing::media_id.eq(id))
                    .select((viewing::watched_seconds, progress::duration_seconds))
                    .first::<(f64, f64)>(connection)
                    .optional()?;
                let value = sample
                    .is_some_and(|(seconds, duration)| seconds > duration * WATCHED_THRESHOLD);
                diesel::insert_into(watched::table)
                    .values((watched::media_id.eq(id), watched::is_watched.eq(value)))
                    .on_conflict(watched::media_id)
                    .do_update()
                    .set((
                        watched::is_watched.eq(value),
                        watched::manual_override.eq(None::<bool>),
                    ))
                    .execute(connection)?;
                Ok(())
            })
    }

    /// # Errors
    /// Returns an error for invalid progress, missing media, or failed writes.
    pub fn save_progress(
        &mut self,
        key: &MediaKey,
        position_seconds: f64,
        duration_seconds: f64,
    ) -> Result<()> {
        ensure!(
            position_seconds.is_finite() && duration_seconds.is_finite() && duration_seconds > 0.0,
            "progress must be finite with a positive duration"
        );
        let position = position_seconds.clamp(0.0, duration_seconds);
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                let id = media_id(connection, key)?;
                save_progress(connection, id, position, duration_seconds)?;
                update_watched(connection, id, is_complete(position, duration_seconds))
            })
    }

    /// # Errors
    /// Returns an error for missing media or failed reads.
    pub fn load_progress(&self, key: &MediaKey) -> Result<Option<Progress>> {
        let connection = &mut *self.connection.borrow_mut();
        let id = media_id(connection, key)?;
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
        ensure!(
            position.is_finite()
                && duration.is_finite()
                && duration > 0.0
                && watched_delta.is_finite()
                && watched_delta >= 0.0,
            "invalid playback sample"
        );
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                let id = media_id(connection, key)?;
                save_progress(connection, id, position.clamp(0.0, duration), duration)?;
                let parent = media::table
                    .find(id)
                    .select(media::parent_id)
                    .first::<Option<i64>>(connection)?;
                if let Some(parent) = parent {
                    let siblings = media::table
                        .filter(media::parent_id.eq(parent))
                        .filter(media::media_id.ne(id))
                        .select(media::media_id);
                    diesel::update(viewing::table.filter(viewing::media_id.eq_any(siblings)))
                        .set(viewing::active.eq(false))
                        .execute(connection)?;
                }
                let total = activate(connection, id, watched_delta)?;
                update_watched(connection, id, total > duration * WATCHED_THRESHOLD)
            })
    }

    /// Reconcile resume state when playback stops beyond the credits threshold.
    /// The caller supplies only a known released next episode.
    /// # Errors
    /// Returns an error for missing media, invalid next identity, or failed writes.
    pub fn finish_playback(&mut self, key: &MediaKey, next: Option<&Media>) -> Result<()> {
        let Some(sample) = self.load_progress(key)? else {
            return Ok(());
        };
        if sample.position_seconds <= sample.duration_seconds * COMPLETION_THRESHOLD {
            return Ok(());
        }
        if let Some(next) = next {
            let current = self.get_media(key)?.context("missing current media")?;
            ensure!(
                key.kind == MediaKind::Episode && next.parent == current.parent && next.key != *key,
                "invalid next episode"
            );
        }
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                let id = media_id(connection, key)?;
                diesel::update(viewing::table.find(id))
                    .set(viewing::active.eq(false))
                    .execute(connection)?;
                diesel::update(progress::table.find(id))
                    .set(progress::position_seconds.eq(0.0))
                    .execute(connection)?;
                if let Some(next) = next {
                    upsert_media(connection, next)?;
                    let next_id = media_id(connection, &next.key)?;
                    diesel::insert_into(progress::table)
                        .values((
                            progress::media_id.eq(next_id),
                            progress::position_seconds.eq(0.0),
                            progress::duration_seconds.eq(1.0),
                        ))
                        .on_conflict(progress::media_id)
                        .do_update()
                        .set(progress::position_seconds.eq(0.0))
                        .execute(connection)?;
                    activate(connection, next_id, 0.0)?;
                }
                Ok(())
            })
    }

    /// # Errors
    /// Returns an error on database reads. One most-recent entry per series.
    pub fn list_currently_watching(&self, page: Page) -> Result<Vec<(Media, Progress)>> {
        let items = {
            let connection = &mut *self.connection.borrow_mut();
            let records = diesel::sql_query("SELECT m.* FROM media m JOIN viewing v ON v.media_id=m.media_id WHERE v.active=1 AND NOT EXISTS(SELECT 1 FROM viewing newer JOIN media nm ON nm.media_id=newer.media_id WHERE newer.active=1 AND m.parent_id IS NOT NULL AND nm.parent_id=m.parent_id AND newer.touched>v.touched) ORDER BY v.touched DESC LIMIT ? OFFSET ?")
                .bind::<diesel::sql_types::BigInt, _>(i64::from(page.limit)).bind::<diesel::sql_types::BigInt, _>(i64::from(page.offset)).load::<MediaRecord>(connection)?;
            hydrate(connection, records)?
        };
        items
            .into_iter()
            .map(|item| {
                let sample = self
                    .load_progress(&item.key)?
                    .context("missing viewing progress")?;
                Ok((item, sample))
            })
            .collect()
    }
}
