use anyhow::{Result, ensure};
use diesel::prelude::*;

use crate::{Library, schema::addons};

/// Stored separately from network types so the database has no HTTP dependency.
#[derive(Clone, Debug, PartialEq, Eq, Queryable, Selectable)]
#[diesel(table_name = addons)]
pub struct AddonRecord {
    pub id: String,
    pub manifest_url: String,
    pub manifest_json: String,
}

impl Library {
    /// # Errors
    /// Returns an error on database reads.
    pub fn load_addons(&self) -> Result<Vec<AddonRecord>> {
        Ok(addons::table
            .order(addons::position)
            .select(AddonRecord::as_select())
            .load(&mut *self.connection.borrow_mut())?)
    }

    /// Atomically replace installed add-ons in display order.
    /// # Errors
    /// Returns an error for invalid or duplicate records, more than 32 add-ons, or failed writes.
    pub fn replace_addons(&mut self, records: &[AddonRecord]) -> Result<()> {
        ensure!(
            records.len() <= 32,
            "at most 32 installed add-ons are supported"
        );
        self.connection
            .borrow_mut()
            .immediate_transaction(|connection| {
                diesel::delete(addons::table).execute(connection)?;
                for (position, record) in records.iter().enumerate() {
                    diesel::insert_into(addons::table)
                        .values((
                            addons::id.eq(&record.id),
                            addons::position.eq(i64::try_from(position)?),
                            addons::manifest_url.eq(&record.manifest_url),
                            addons::manifest_json.eq(&record.manifest_json),
                        ))
                        .execute(connection)?;
                }
                Ok(())
            })
    }
}
