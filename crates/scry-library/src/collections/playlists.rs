use std::collections::HashSet;

use anyhow::{Result, ensure};
use diesel::{dsl::max, prelude::*};

use crate::{
    Library, Media, MediaKey, Page, Playlist,
    catalog::media::{MediaRecord, hydrate, media_id},
    storage::schema::{media, playlist_items, playlists},
};

fn playlist_exists(connection: &mut SqliteConnection, id: i64) -> Result<()> {
    ensure!(
        diesel::select(diesel::dsl::exists(playlists::table.find(id)))
            .get_result::<bool>(connection)?,
        "playlist not found"
    );
    Ok(())
}

impl Library {
    /// # Errors
    /// Returns an error for an invalid name or failed writes.
    pub fn create_playlist(&mut self, name: &str) -> Result<Playlist> {
        ensure!(!name.trim().is_empty(), "playlist name cannot be empty");
        let id = diesel::insert_into(playlists::table)
            .values(playlists::name.eq(name))
            .returning(playlists::playlist_id)
            .get_result(&mut *self.connection.borrow_mut())?;
        Ok(Playlist {
            id,
            name: name.into(),
        })
    }

    /// # Errors
    /// Returns an error for an invalid name, missing playlist, or failed writes.
    pub fn rename_playlist(&mut self, id: i64, name: &str) -> Result<()> {
        ensure!(!name.trim().is_empty(), "playlist name cannot be empty");
        ensure!(
            diesel::update(playlists::table.find(id))
                .set(playlists::name.eq(name))
                .execute(&mut *self.connection.borrow_mut())?
                == 1,
            "playlist not found"
        );
        Ok(())
    }

    /// # Errors
    /// Returns an error on database writes.
    pub fn delete_playlist(&mut self, id: i64) -> Result<()> {
        diesel::delete(playlists::table.find(id)).execute(&mut *self.connection.borrow_mut())?;
        Ok(())
    }

    /// # Errors
    /// Returns an error on database reads.
    pub fn list_playlists(&self, page: Page) -> Result<Vec<Playlist>> {
        let records = playlists::table
            .order(playlists::playlist_id)
            .limit(i64::from(page.limit))
            .offset(i64::from(page.offset))
            .load::<(i64, String)>(&mut *self.connection.borrow_mut())?;
        Ok(records
            .into_iter()
            .map(|(id, name)| Playlist { id, name })
            .collect())
    }

    /// # Errors
    /// Returns an error for missing playlist or media, or failed writes.
    pub fn add_playlist_item(&mut self, playlist: i64, key: &MediaKey) -> Result<()> {
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                playlist_exists(connection, playlist)?;
                let id = media_id(connection, key)?;
                let last: Option<i64> = playlist_items::table
                    .filter(playlist_items::playlist_id.eq(playlist))
                    .select(max(playlist_items::position))
                    .first(connection)?;
                let position = last.map_or(Ok(0), |value| {
                    value
                        .checked_add(1)
                        .ok_or_else(|| anyhow::anyhow!("playlist position overflow"))
                })?;
                diesel::insert_into(playlist_items::table)
                    .values((
                        playlist_items::playlist_id.eq(playlist),
                        playlist_items::media_id.eq(id),
                        playlist_items::position.eq(position),
                    ))
                    .on_conflict((playlist_items::playlist_id, playlist_items::media_id))
                    .do_nothing()
                    .execute(connection)?;
                Ok(())
            })
    }

    /// # Errors
    /// Returns an error for missing playlist or media, or failed writes.
    pub fn remove_playlist_item(&mut self, playlist: i64, key: &MediaKey) -> Result<()> {
        let connection = &mut *self.connection.borrow_mut();
        playlist_exists(connection, playlist)?;
        let id = media_id(connection, key)?;
        diesel::delete(playlist_items::table.find((playlist, id))).execute(connection)?;
        Ok(())
    }

    /// Replace the order, not membership. Requires each current item exactly once.
    /// # Errors
    /// Returns an error for incomplete or duplicate membership, or failed writes.
    pub fn reorder_playlist(&mut self, playlist: i64, keys: &[MediaKey]) -> Result<()> {
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                playlist_exists(connection, playlist)?;
                let current: HashSet<i64> = playlist_items::table
                    .filter(playlist_items::playlist_id.eq(playlist))
                    .select(playlist_items::media_id)
                    .load::<i64>(connection)?
                    .into_iter()
                    .collect();
                ensure!(
                    keys.len() == current.len(),
                    "reorder must include every playlist item once"
                );
                let ordered = keys
                    .iter()
                    .map(|key| media_id(connection, key))
                    .collect::<Result<Vec<_>>>()?;
                let proposed: HashSet<_> = ordered.iter().copied().collect();
                ensure!(
                    proposed == current && ordered.len() == proposed.len(),
                    "reorder must include every playlist item once"
                );
                diesel::delete(
                    playlist_items::table.filter(playlist_items::playlist_id.eq(playlist)),
                )
                .execute(connection)?;
                for (position, id) in ordered.iter().enumerate() {
                    diesel::insert_into(playlist_items::table)
                        .values((
                            playlist_items::playlist_id.eq(playlist),
                            playlist_items::media_id.eq(id),
                            playlist_items::position.eq(i64::try_from(position)?),
                        ))
                        .execute(connection)?;
                }
                Ok(())
            })
    }

    /// # Errors
    /// Returns an error for a missing playlist or failed reads.
    pub fn list_playlist_items(&self, playlist: i64, page: Page) -> Result<Vec<Media>> {
        let connection = &mut *self.connection.borrow_mut();
        playlist_exists(connection, playlist)?;
        let records = media::table
            .inner_join(playlist_items::table)
            .filter(playlist_items::playlist_id.eq(playlist))
            .order(playlist_items::position)
            .limit(i64::from(page.limit))
            .offset(i64::from(page.offset))
            .select(MediaRecord::as_select())
            .load(connection)?;
        hydrate(connection, records)
    }
}
