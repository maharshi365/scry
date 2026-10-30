//! Persistent, provider-scoped episode segment responses. Network policy lives in the app.
use anyhow::{Result, ensure};
use diesel::{
    prelude::*,
    sql_types::{BigInt, Nullable, Text},
};

use crate::Library;

/// A saved provider response, including metadata for conditional refreshes.
#[derive(Clone, Debug, PartialEq, Eq, QueryableByName)]
pub struct SkipSegmentsRecord {
    #[diesel(sql_type = Text)]
    pub payload: String,
    #[diesel(sql_type = Nullable<Text>)]
    pub etag: Option<String>,
    #[diesel(sql_type = BigInt)]
    pub fetched_at: i64,
}

impl Library {
    /// Load one episode's saved segments without applying network freshness policy.
    /// # Errors
    /// Returns an error if the database cannot be read.
    pub fn skip_segments(
        &self,
        provider: &str,
        imdb_id: &str,
        season: u32,
        episode: u32,
    ) -> Result<Option<SkipSegmentsRecord>> {
        Ok(diesel::sql_query("SELECT payload, etag, fetched_at FROM skip_segments WHERE provider = ? AND imdb_id = ? AND season = ? AND episode = ?")
            .bind::<Text, _>(provider)
            .bind::<Text, _>(imdb_id)
            .bind::<BigInt, _>(i64::from(season))
            .bind::<BigInt, _>(i64::from(episode))
            .get_result(&mut *self.connection.borrow_mut()).optional()?)
    }

    /// Replace one episode's response atomically; other providers and episodes are untouched.
    /// # Errors
    /// Returns an error for invalid or oversized records, or a failed database write.
    pub fn save_skip_segments(
        &self,
        provider: &str,
        imdb_id: &str,
        season: u32,
        episode: u32,
        record: &SkipSegmentsRecord,
    ) -> Result<()> {
        ensure!(
            !provider.is_empty()
                && provider.len() <= 64
                && !imdb_id.is_empty()
                && imdb_id.len() <= 32,
            "invalid skip segment identity"
        );
        ensure!(
            episode > 0
                && record.fetched_at >= 0
                && record.payload.len() <= 65536
                && record.etag.as_ref().is_none_or(|tag| tag.len() <= 512),
            "invalid skip segment record"
        );
        diesel::sql_query("INSERT INTO skip_segments (provider, imdb_id, season, episode, payload, etag, fetched_at) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (provider, imdb_id, season, episode) DO UPDATE SET payload = excluded.payload, etag = excluded.etag, fetched_at = excluded.fetched_at")
            .bind::<Text, _>(provider)
            .bind::<Text, _>(imdb_id)
            .bind::<BigInt, _>(i64::from(season))
            .bind::<BigInt, _>(i64::from(episode))
            .bind::<Text, _>(&record.payload)
            .bind::<Nullable<Text>, _>(&record.etag)
            .bind::<BigInt, _>(record.fetched_at)
            .execute(&mut *self.connection.borrow_mut())?;
        Ok(())
    }
}

pub(super) fn validate(connection: &mut SqliteConnection) -> Result<()> {
    diesel::sql_query("SELECT payload, etag, fetched_at FROM skip_segments LIMIT 0")
        .load::<SkipSegmentsRecord>(connection)?;
    Ok(())
}
