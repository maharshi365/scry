//! Synchronous, local-only app storage. Run on a worker, never the UI thread.
mod app_storage;
mod media;
mod models;
mod playback;
mod playlists;
mod schema;

pub use app_storage::AddonRecord;
pub use models::*;

use std::{cell::RefCell, path::Path};

use anyhow::{Context, Result, ensure};
use diesel::{
    connection::SimpleConnection,
    migration::MigrationSource,
    prelude::*,
    sql_types::{BigInt, Text},
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

/// All persistent app state shares this database. Older pre-release stores are not imported.
pub const DATABASE_NAME: &str = "scry.sqlite3";
const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub struct Library {
    connection: RefCell<SqliteConnection>,
}

impl Library {
    /// # Errors
    /// Returns an error if the database cannot be opened, migrated, or validated.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path
            .as_ref()
            .to_str()
            .context("database path is not UTF-8")?;
        Self::initialize(SqliteConnection::establish(path).context("open local app database")?)
    }

    /// # Errors
    /// Returns an error if an in-memory database cannot be initialized.
    pub fn memory() -> Result<Self> {
        Self::initialize(SqliteConnection::establish(":memory:")?)
    }

    fn initialize(mut connection: SqliteConnection) -> Result<Self> {
        connection.batch_execute(
            "PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;",
        )?;
        // Serialize startup across the library and add-on workers.
        connection.immediate_transaction::<_, anyhow::Error, _>(|connection| {
            let known = <EmbeddedMigrations as MigrationSource<diesel::sqlite::Sqlite>>::migrations(&MIGRATIONS)
                .map_err(|error| anyhow::anyhow!("read embedded migrations: {error}"))?;
            let applied = connection.applied_migrations()
                .map_err(|error| anyhow::anyhow!("read migration history: {error}"))?;
            if applied.is_empty() {
                #[derive(QueryableByName)]
                struct Count {
                    #[diesel(sql_type = BigInt)]
                    count: i64,
                }
                let tables = diesel::sql_query("SELECT count(*) AS count FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' AND name != '__diesel_schema_migrations'")
                    .get_result::<Count>(connection)?;
                ensure!(tables.count == 0, "unversioned app database is not empty");
            }
            ensure!(applied.iter().all(|version| known.iter().any(|migration| migration.name().version() == *version)),
                "unsupported app database migration version");
            connection.run_pending_migrations(MIGRATIONS)
                .map_err(|error| anyhow::anyhow!("migrate app database: {error}"))?;
            Self::validate(connection)?;
            Ok(())
        })?;
        Ok(Self {
            connection: RefCell::new(connection),
        })
    }

    fn validate(connection: &mut SqliteConnection) -> Result<()> {
        #[derive(QueryableByName)]
        struct ForeignKeyViolation {
            #[diesel(sql_type = Text, column_name = "table")]
            _table: String,
        }
        // Check every mapped column without loading data. Do not duplicate migration SQL.
        schema::media::table
            .limit(0)
            .load::<media::MediaRecord>(connection)?;
        schema::watchlist::table
            .limit(0)
            .load::<(i64, i64)>(connection)?;
        schema::playlists::table
            .limit(0)
            .load::<(i64, String)>(connection)?;
        schema::playlist_items::table
            .limit(0)
            .load::<(i64, i64, i64)>(connection)?;
        schema::progress::table
            .limit(0)
            .load::<(i64, f64, f64)>(connection)?;
        schema::watched::table
            .limit(0)
            .load::<(i64, bool, Option<bool>)>(connection)?;
        schema::viewing::table
            .limit(0)
            .load::<(i64, f64, bool, i64)>(connection)?;
        schema::addons::table
            .limit(0)
            .load::<(String, i64, String, String)>(connection)?;
        ensure!(
            diesel::sql_query("PRAGMA foreign_key_check")
                .load::<ForeignKeyViolation>(connection)?
                .is_empty(),
            "app database contains invalid foreign keys"
        );
        Ok(())
    }
}
