use anyhow::{Result, ensure};
use diesel::{prelude::*, sql_types::Text};

use crate::{Library, MediaKey};

impl Library {
    /// Store the last working stream without interpreting provider-specific fields.
    /// # Errors
    /// Returns an error for oversized records, unknown media, or failed writes.
    pub fn save_stream(&mut self, key: &MediaKey, stream_json: &str) -> Result<()> {
        ensure!(stream_json.len() <= 65_536, "saved stream is too large");
        let connection = &mut *self.connection.borrow_mut();
        let id = crate::catalog::media::media_id(connection, key)?;
        diesel::sql_query("INSERT INTO saved_streams (media_id, stream_json) VALUES (?, ?) ON CONFLICT(media_id) DO UPDATE SET stream_json=excluded.stream_json")
            .bind::<diesel::sql_types::BigInt, _>(id)
            .bind::<Text, _>(stream_json)
            .execute(connection)?;
        Ok(())
    }

    /// Load the last working stream for the exact media identity.
    /// # Errors
    /// Returns an error if the database cannot be read.
    pub fn saved_stream(&self, key: &MediaKey) -> Result<Option<String>> {
        #[derive(QueryableByName)]
        struct Record {
            #[diesel(sql_type = Text)]
            stream_json: String,
        }
        Ok(diesel::sql_query("SELECT stream_json FROM saved_streams JOIN media USING(media_id) WHERE namespace=? AND kind=? AND external_id=?")
            .bind::<Text, _>(&key.namespace)
            .bind::<Text, _>(key.kind.as_str())
            .bind::<Text, _>(&key.id)
            .get_result::<Record>(&mut *self.connection.borrow_mut())
            .optional()?
            .map(|record| record.stream_json))
    }
}
