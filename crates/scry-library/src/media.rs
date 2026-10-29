use std::collections::HashMap;

use anyhow::{Context, Result, ensure};
use diesel::{prelude::*, upsert::excluded};

use crate::{
    Library, Media, MediaKey, MediaKind, Page, Progress,
    schema::{effective_watched, media, watchlist},
};

#[derive(Queryable, Selectable, QueryableByName)]
#[diesel(table_name = media)]
pub(crate) struct MediaRecord {
    media_id: i64,
    namespace: String,
    kind: String,
    external_id: String,
    title: String,
    parent_id: Option<i64>,
}

#[derive(QueryableByName)]
pub(crate) struct MediaProgressRecord {
    #[diesel(embed)]
    media: MediaRecord,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Double>)]
    position_seconds: Option<f64>,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Double>)]
    duration_seconds: Option<f64>,
}

pub(crate) fn hydrate_progress(
    connection: &mut SqliteConnection,
    records: Vec<MediaProgressRecord>,
) -> Result<Vec<(Media, Option<Progress>)>> {
    let (media, progress): (Vec<_>, Vec<_>) = records
        .into_iter()
        .map(|record| {
            (
                record.media,
                record.position_seconds.zip(record.duration_seconds).map(
                    |(position_seconds, duration_seconds)| Progress {
                        position_seconds,
                        duration_seconds,
                    },
                ),
            )
        })
        .unzip();
    Ok(hydrate(connection, media)?
        .into_iter()
        .zip(progress)
        .collect())
}

impl MediaRecord {
    fn into_key(self) -> Result<MediaKey> {
        Ok(MediaKey::new(
            self.namespace,
            MediaKind::parse(&self.kind)?,
            self.external_id,
        ))
    }
}

pub(crate) fn media_id(connection: &mut SqliteConnection, key: &MediaKey) -> Result<i64> {
    media::table
        .filter(media::namespace.eq(&key.namespace))
        .filter(media::kind.eq(key.kind.as_str()))
        .filter(media::external_id.eq(&key.id))
        .select(media::media_id)
        .first(connection)
        .optional()?
        .context("media is not in the library; upsert metadata first")
}

pub(crate) fn hydrate(
    connection: &mut SqliteConnection,
    records: Vec<MediaRecord>,
) -> Result<Vec<Media>> {
    let parent_ids: Vec<_> = records
        .iter()
        .filter_map(|record| record.parent_id)
        .collect();
    let mut parents = HashMap::new();
    if !parent_ids.is_empty() {
        for parent in media::table
            .filter(media::media_id.eq_any(parent_ids))
            .select(MediaRecord::as_select())
            .load(connection)?
        {
            parents.insert(parent.media_id, parent.into_key()?);
        }
    }
    records
        .into_iter()
        .map(|record| {
            Ok(Media {
                key: MediaKey::new(
                    record.namespace,
                    MediaKind::parse(&record.kind)?,
                    record.external_id,
                ),
                title: record.title,
                parent: record
                    .parent_id
                    .map(|id| parents.get(&id).cloned().context("missing media parent"))
                    .transpose()?,
            })
        })
        .collect()
}

pub(crate) fn validate_media(item: &Media) -> Result<()> {
    ensure!(
        !item.key.namespace.trim().is_empty() && !item.key.id.trim().is_empty(),
        "media identity cannot be empty"
    );
    ensure!(!item.title.trim().is_empty(), "media title cannot be empty");
    if let Some(parent) = &item.parent {
        ensure!(
            item.key.kind == MediaKind::Episode && parent.kind == MediaKind::Series,
            "only episodes can link to a parent series"
        );
    }
    Ok(())
}

pub(crate) fn upsert_media(connection: &mut SqliteConnection, item: &Media) -> Result<()> {
    validate_media(item)?;
    let parent_id = if let Some(parent) = &item.parent {
        Some(media_id(connection, parent)?)
    } else {
        None
    };
    diesel::insert_into(media::table)
        .values((
            media::namespace.eq(&item.key.namespace),
            media::kind.eq(item.key.kind.as_str()),
            media::external_id.eq(&item.key.id),
            media::title.eq(&item.title),
            media::parent_id.eq(parent_id),
        ))
        .on_conflict((media::namespace, media::kind, media::external_id))
        .do_update()
        .set((
            media::title.eq(excluded(media::title)),
            media::parent_id.eq(excluded(media::parent_id)),
        ))
        .execute(connection)?;
    Ok(())
}

impl Library {
    /// Register a bounded batch atomically, with parents before their episodes.
    /// # Errors
    /// Returns an error for an oversized batch, invalid metadata, or failed writes.
    pub fn upsert_media_batch(&mut self, items: &[Media]) -> Result<()> {
        ensure!(items.len() <= 50_000, "too many media records");
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                for item in items {
                    upsert_media(connection, item)?;
                }
                Ok(())
            })
    }

    /// # Errors
    /// Returns an error for invalid metadata, missing parent, or failed writes.
    pub fn upsert_media(&mut self, item: &Media) -> Result<()> {
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| upsert_media(connection, item))
    }

    /// # Errors
    /// Returns an error on database reads.
    pub fn get_media(&self, key: &MediaKey) -> Result<Option<Media>> {
        let connection = &mut *self.connection.borrow_mut();
        let records = media::table
            .filter(media::namespace.eq(&key.namespace))
            .filter(media::kind.eq(key.kind.as_str()))
            .filter(media::external_id.eq(&key.id))
            .select(MediaRecord::as_select())
            .load(connection)?;
        Ok(hydrate(connection, records)?.into_iter().next())
    }

    /// # Errors
    /// Returns an error for missing media or a failed write.
    pub fn add_to_watchlist(&mut self, key: &MediaKey) -> Result<()> {
        let connection = &mut *self.connection.borrow_mut();
        let id = media_id(connection, key)?;
        crate::watchlist::set(connection, id, true)
    }

    /// # Errors
    /// Returns an error for missing media or a failed write.
    pub fn remove_from_watchlist(&mut self, key: &MediaKey) -> Result<()> {
        let connection = &mut *self.connection.borrow_mut();
        let id = media_id(connection, key)?;
        crate::watchlist::set(connection, id, false)
    }

    /// # Errors
    /// Returns an error for missing media or a failed read.
    pub fn is_in_watchlist(&self, key: &MediaKey) -> Result<bool> {
        let connection = &mut *self.connection.borrow_mut();
        let id = media_id(connection, key)?;
        crate::watchlist::contains(connection, id)
    }

    /// # Errors
    /// Returns an error on database reads.
    pub fn list_watchlist(&self, page: Page) -> Result<Vec<Media>> {
        let connection = &mut *self.connection.borrow_mut();
        let records = media::table
            .inner_join(watchlist::table)
            .order(watchlist::entry_id.desc())
            .limit(i64::from(page.limit))
            .offset(i64::from(page.offset))
            .select(MediaRecord::as_select())
            .load(connection)?;
        hydrate(connection, records)
    }

    /// # Errors
    /// Returns an error for an invalid series or failed reads.
    pub fn list_episodes(&self, series: &MediaKey, page: Page) -> Result<Vec<Media>> {
        ensure!(series.kind == MediaKind::Series, "expected a series key");
        let connection = &mut *self.connection.borrow_mut();
        let id = media_id(connection, series)?;
        let records = media::table
            .filter(media::parent_id.eq(id))
            .order(media::media_id)
            .limit(i64::from(page.limit))
            .offset(i64::from(page.offset))
            .select(MediaRecord::as_select())
            .load(connection)?;
        hydrate(connection, records)
    }

    /// # Errors
    /// Returns an error on database reads.
    pub fn list_watched(&self, page: Page) -> Result<Vec<Media>> {
        let connection = &mut *self.connection.borrow_mut();
        let records = media::table
            .inner_join(effective_watched::table)
            .filter(effective_watched::is_watched.eq(true))
            .order(media::media_id.desc())
            .limit(i64::from(page.limit))
            .offset(i64::from(page.offset))
            .select(MediaRecord::as_select())
            .load(connection)?;
        hydrate(connection, records)
    }

    /// # Errors
    /// Returns an error on database reads. Filter is applied before pagination.
    pub fn list_library(
        &self,
        is_watched: bool,
        kind: MediaKind,
        page: Page,
    ) -> Result<Vec<Media>> {
        let connection = &mut *self.connection.borrow_mut();
        let complete = crate::watch_status::completion_sql("m.media_id", "unixepoch()");
        let records = diesel::sql_query(format!(
            "SELECT m.* FROM media m JOIN watchlist s ON s.media_id=m.media_id LEFT JOIN watched v ON v.media_id=m.media_id WHERE m.kind=? AND (CASE WHEN m.kind='series' THEN ({complete}) ELSE COALESCE(v.watched,0) END)=? ORDER BY m.media_id DESC LIMIT ? OFFSET ?"
        ))
            .bind::<diesel::sql_types::Text, _>(kind.as_str())
            .bind::<diesel::sql_types::Bool, _>(is_watched)
            .bind::<diesel::sql_types::BigInt, _>(i64::from(page.limit))
            .bind::<diesel::sql_types::BigInt, _>(i64::from(page.offset))
            .load(connection)?;
        hydrate(connection, records)
    }

    /// Only movies and series explicitly saved in the library, regardless of watch state.
    /// # Errors
    /// Returns an error on database reads.
    pub fn list_calendar_titles(&self) -> Result<Vec<Media>> {
        let connection = &mut *self.connection.borrow_mut();
        let records = media::table
            .inner_join(watchlist::table)
            .filter(media::kind.eq_any([MediaKind::Movie.as_str(), MediaKind::Series.as_str()]))
            .order(media::media_id.desc())
            .select(MediaRecord::as_select())
            .load(connection)?;
        hydrate(connection, records)
    }

    /// Shows saved in the library or with episode watch/progress state, independent of list filters.
    /// # Errors
    /// Returns an error on database reads.
    pub fn list_tracked_series(&self, page: Page) -> Result<Vec<Media>> {
        let connection = &mut *self.connection.borrow_mut();
        let records = diesel::sql_query("SELECT m.* FROM media m WHERE m.kind='series' AND (EXISTS(SELECT 1 FROM watchlist w WHERE w.media_id=m.media_id) OR EXISTS(SELECT 1 FROM watched w WHERE w.media_id=m.media_id) OR EXISTS(SELECT 1 FROM media e WHERE e.parent_id=m.media_id AND (EXISTS(SELECT 1 FROM watched w WHERE w.media_id=e.media_id) OR EXISTS(SELECT 1 FROM viewing v WHERE v.media_id=e.media_id)))) ORDER BY m.media_id DESC LIMIT ? OFFSET ?")
            .bind::<diesel::sql_types::BigInt, _>(i64::from(page.limit))
            .bind::<diesel::sql_types::BigInt, _>(i64::from(page.offset)).load(connection)?;
        hydrate(connection, records)
    }
}
