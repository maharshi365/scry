//! Synchronous, local-only app storage. Run on a worker, never the UI thread.
#![forbid(unsafe_code)]

mod achievements;
mod app_storage;
pub mod guide;
mod media;
mod models;
mod playback;
mod playlists;
mod schema;
mod stats;
mod up_next;
mod watch_status;
mod watchlist;

pub use achievements::{
    Achievement, AchievementDb, AchievementEntry, AchievementEvaluation, Evaluation,
};
pub use app_storage::AddonRecord;
pub use models::*;
pub use stats::{NerdStats, StatsPeriod, TopTitle};
pub use watch_status::can_mark_watched;

use std::{cell::RefCell, path::Path, sync::Mutex};

use anyhow::{Context, Result, ensure};
use diesel::{
    connection::SimpleConnection,
    prelude::*,
    sql_types::{BigInt, Text},
};

/// All persistent app state shares this database. Older pre-release stores are not imported.
pub const DATABASE_NAME: &str = "scry-v2.sqlite3";
const SCHEMA_VERSION: i64 = 6;
// SQLite's initial journal-mode switch can return BUSY without using the
// busy timeout. Serialize our workers' startup, not their normal database work.
static INITIALIZE: Mutex<()> = Mutex::new(());

pub struct Library {
    connection: RefCell<SqliteConnection>,
}

impl Library {
    /// # Errors
    /// Returns an error if the database cannot be opened, initialized, or validated.
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
        let _initializing = INITIALIZE
            .lock()
            .map_err(|_| anyhow::anyhow!("database initialization lock is poisoned"))?;
        connection.batch_execute(
            "PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;",
        )?;
        // Serialize startup across the library and add-on workers.
        connection.immediate_transaction::<_, anyhow::Error, _>(|connection| {
            #[derive(QueryableByName)]
            struct Version {
                #[diesel(sql_type = BigInt)]
                user_version: i64,
            }
            let version = diesel::sql_query("PRAGMA user_version")
                .get_result::<Version>(connection)?
                .user_version;
            if version == 0 {
                #[derive(QueryableByName)]
                struct Count {
                    #[diesel(sql_type = BigInt)]
                    count: i64,
                }
                let tables = diesel::sql_query(
                    "SELECT count(*) AS count FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                )
                .get_result::<Count>(connection)?;
                ensure!(tables.count == 0, "unversioned app database is not empty");
                connection.batch_execute(include_str!("schema.sql"))?;
                watch_status::install_view(connection)?;
                connection.batch_execute("PRAGMA user_version=4;")?;
            } else if version == 1 {
                connection.batch_execute(include_str!("migrations/2.sql"))?;
                watch_status::install_view(connection)?;
                connection.batch_execute("PRAGMA user_version=4;")?;
            } else if version == 2 || version == 3 {
                connection.batch_execute("DROP VIEW effective_watched;")?;
                watch_status::install_view(connection)?;
                connection.batch_execute("PRAGMA user_version=4;")?;
            } else if version != 4 && version != 5 {
                ensure!(
                    version == SCHEMA_VERSION,
                    "unsupported app database schema; reset the local database"
                );
            }
            if version < 5 {
                connection.batch_execute(include_str!("stats_schema.sql"))?;
                connection.batch_execute("PRAGMA user_version=5;")?;
            }
            if version < 6 {
                connection.batch_execute(include_str!("guide_schema.sql"))?;
                connection.batch_execute("PRAGMA user_version=6;")?;
            }
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
        // Check every mapped column without loading data.
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
            .load::<(i64, bool)>(connection)?;
        schema::viewing::table
            .limit(0)
            .load::<(i64, f64, bool, i64)>(connection)?;
        schema::addons::table
            .limit(0)
            .load::<(String, i64, String, String)>(connection)?;
        schema::episode_releases::table
            .limit(0)
            .load::<(i64, Option<i64>, Option<i64>, Option<i64>)>(connection)?;
        schema::series_inventory::table
            .limit(0)
            .load::<(i64,)>(connection)?;
        schema::effective_watched::table
            .limit(0)
            .load::<(i64, bool)>(connection)?;
        stats::validate(connection)?;
        guide::validate(connection)?;
        ensure!(
            diesel::sql_query("PRAGMA foreign_key_check")
                .load::<ForeignKeyViolation>(connection)?
                .is_empty(),
            "app database contains invalid foreign keys"
        );
        Ok(())
    }
}
