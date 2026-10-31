use anyhow::Result;
use diesel::prelude::*;

use crate::{
    Library, Media,
    catalog::media::{media_id, upsert_media, validate_media},
    storage::schema::watchlist,
};

pub(crate) fn contains(connection: &mut SqliteConnection, id: i64) -> Result<bool> {
    Ok(diesel::select(diesel::dsl::exists(
        watchlist::table.filter(watchlist::media_id.eq(id)),
    ))
    .get_result(connection)?)
}

pub(crate) fn set(connection: &mut SqliteConnection, id: i64, added: bool) -> Result<()> {
    if added {
        diesel::insert_into(watchlist::table)
            .values(watchlist::media_id.eq(id))
            .on_conflict(watchlist::media_id)
            .do_nothing()
            .execute(connection)?;
    } else {
        diesel::delete(watchlist::table.filter(watchlist::media_id.eq(id))).execute(connection)?;
    }
    Ok(())
}

impl Library {
    /// Resolve an episode to its stored series and update library membership atomically.
    /// `None` toggles membership; `Some` sets the requested membership.
    /// # Errors
    /// Returns an error for invalid metadata, missing parents, or failed reads/writes.
    pub fn update_media_watchlist(&mut self, item: &Media, added: Option<bool>) -> Result<()> {
        validate_media(item)?;
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                let id = if let Some(parent) = &item.parent {
                    media_id(connection, parent)?
                } else {
                    upsert_media(connection, item)?;
                    media_id(connection, &item.key)?
                };
                let added = match added {
                    Some(added) => added,
                    None => !contains(connection, id)?,
                };
                set(connection, id, added)
            })
    }
}
