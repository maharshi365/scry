//! Synchronous, local-only library storage. Run on a dedicated worker, never the UI thread.

use std::{collections::HashSet, path::Path, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

pub const MAX_PAGE_SIZE: u32 = 200;
pub const COMPLETION_THRESHOLD: f64 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Movie,
    Series,
    Episode,
}

impl MediaKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Series => "series",
            Self::Episode => "episode",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "movie" => Ok(Self::Movie),
            "series" => Ok(Self::Series),
            "episode" => Ok(Self::Episode),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MediaKey {
    pub namespace: String,
    pub kind: MediaKind,
    pub id: String,
}

impl MediaKey {
    pub fn new(namespace: impl Into<String>, kind: MediaKind, id: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            kind,
            id: id.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Media {
    pub key: MediaKey,
    pub title: String,
    pub parent: Option<MediaKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub position_seconds: f64,
    pub duration_seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchedState {
    pub watched: bool,
    pub manual_override: Option<bool>,
}

#[derive(Clone, Copy, Debug)]
pub struct Page {
    limit: u32,
    offset: u32,
}

impl Page {
    pub fn new(limit: u32, offset: u32) -> Result<Self> {
        ensure!(
            (1..=MAX_PAGE_SIZE).contains(&limit),
            "page limit must be 1..={MAX_PAGE_SIZE}"
        );
        Ok(Self { limit, offset })
    }
}

impl Default for Page {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
        }
    }
}

pub fn is_complete(position_seconds: f64, duration_seconds: f64) -> bool {
    position_seconds.is_finite()
        && duration_seconds.is_finite()
        && duration_seconds > 0.0
        && position_seconds / duration_seconds >= COMPLETION_THRESHOLD
}

pub struct Library {
    connection: Connection,
}

const SCHEMA: &[(&str, &str)] = &[
    ("media", "CREATE TABLE media (
        media_id INTEGER PRIMARY KEY,
        namespace TEXT NOT NULL CHECK(length(trim(namespace)) > 0),
        kind TEXT NOT NULL CHECK(kind IN ('movie','series','episode')),
        external_id TEXT NOT NULL CHECK(length(trim(external_id)) > 0),
        title TEXT NOT NULL CHECK(length(trim(title)) > 0),
        parent_id INTEGER REFERENCES media(media_id),
        CHECK(parent_id IS NULL OR kind = 'episode'),
        UNIQUE(namespace, kind, external_id)
    )"),
    ("watchlist", "CREATE TABLE watchlist (
        entry_id INTEGER PRIMARY KEY AUTOINCREMENT,
        media_id INTEGER NOT NULL UNIQUE REFERENCES media(media_id) ON DELETE CASCADE
    )"),
    ("playlists", "CREATE TABLE playlists (
        playlist_id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL CHECK(length(trim(name)) > 0)
    )"),
    ("playlist_items", "CREATE TABLE playlist_items (
        playlist_id INTEGER NOT NULL REFERENCES playlists(playlist_id) ON DELETE CASCADE,
        media_id INTEGER NOT NULL REFERENCES media(media_id) ON DELETE CASCADE,
        position INTEGER NOT NULL CHECK(position >= 0),
        PRIMARY KEY(playlist_id, media_id),
        UNIQUE(playlist_id, position)
    )"),
    ("progress", "CREATE TABLE progress (
        media_id INTEGER PRIMARY KEY REFERENCES media(media_id) ON DELETE CASCADE,
        position_seconds REAL NOT NULL CHECK(position_seconds >= 0 AND position_seconds <= duration_seconds),
        duration_seconds REAL NOT NULL CHECK(duration_seconds > 0)
    )"),
    ("watched", "CREATE TABLE watched (
        media_id INTEGER PRIMARY KEY REFERENCES media(media_id) ON DELETE CASCADE,
        watched INTEGER NOT NULL CHECK(watched IN (0,1)),
        manual_override INTEGER CHECK(manual_override IN (0,1))
    )"),
    ("media_parent", "CREATE INDEX media_parent ON media(parent_id, media_id)"),
    ("watched_status", "CREATE INDEX watched_status ON watched(watched, media_id)"),
];

const MEDIA_SELECT: &str = "SELECT m.namespace,m.kind,m.external_id,m.title,p.namespace,p.kind,p.external_id FROM media m LEFT JOIN media p ON p.media_id=m.parent_id";

fn read_media(row: &rusqlite::Row<'_>) -> rusqlite::Result<Media> {
    let parent_namespace: Option<String> = row.get(4)?;
    Ok(Media {
        key: MediaKey::new(
            row.get::<_, String>(0)?,
            MediaKind::parse(&row.get::<_, String>(1)?)?,
            row.get::<_, String>(2)?,
        ),
        title: row.get(3)?,
        parent: match parent_namespace {
            Some(namespace) => Some(MediaKey::new(
                namespace,
                MediaKind::parse(&row.get::<_, String>(5)?)?,
                row.get::<_, String>(6)?,
            )),
            None => None,
        },
    })
}

fn media_id(connection: &Connection, key: &MediaKey) -> Result<i64> {
    connection
        .query_row(
            "SELECT media_id FROM media WHERE namespace=?1 AND kind=?2 AND external_id=?3",
            params![key.namespace, key.kind.as_str(), key.id],
            |row| row.get(0),
        )
        .optional()?
        .context("media is not in the library; upsert metadata first")
}

fn playlist_exists(connection: &Connection, id: i64) -> Result<()> {
    ensure!(
        connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM playlists WHERE playlist_id=?1)",
            [id],
            |r| r.get::<_, bool>(0)
        )?,
        "playlist not found"
    );
    Ok(())
}

impl Library {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::initialize(Connection::open(path).context("open local library")?)
    }

    pub fn memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }

    fn initialize(mut connection: Connection) -> Result<Self> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => {
                let count: i64 = tx.query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                    [],
                    |r| r.get(0),
                )?;
                ensure!(
                    count == 0,
                    "unversioned database is not empty; migrations are not supported"
                );
                for (_, sql) in SCHEMA {
                    tx.execute_batch(sql)?;
                }
                tx.pragma_update(None, "user_version", 1)?;
            }
            1 => {}
            _ => bail!("unsupported library schema version {version}; expected 1"),
        }
        // Check table constraints and indexes too, not just whether SELECT succeeds.
        let normalize = |sql: &str| {
            sql.chars()
                .filter(|c| !c.is_whitespace() && *c != ';')
                .collect::<String>()
        };
        for (name, expected) in SCHEMA {
            let actual: Option<String> = tx
                .query_row("SELECT sql FROM sqlite_schema WHERE name=?1", [name], |r| {
                    r.get(0)
                })
                .optional()?;
            ensure!(
                actual.as_deref().map(&normalize) == Some(normalize(expected)),
                "invalid library schema: {name}"
            );
        }
        let invalid = tx.prepare("PRAGMA foreign_key_check")?.exists([])?;
        ensure!(!invalid, "library contains invalid foreign keys");
        tx.commit()?;
        Ok(Self { connection })
    }

    pub fn upsert_media(&mut self, media: &Media) -> Result<()> {
        ensure!(
            !media.key.namespace.trim().is_empty() && !media.key.id.trim().is_empty(),
            "media identity cannot be empty"
        );
        ensure!(
            !media.title.trim().is_empty(),
            "media title cannot be empty"
        );
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let parent_id = if let Some(parent) = &media.parent {
            ensure!(
                media.key.kind == MediaKind::Episode && parent.kind == MediaKind::Series,
                "only episodes can link to a parent series"
            );
            Some(media_id(&tx, parent)?)
        } else {
            None
        };
        tx.execute("INSERT INTO media(namespace,kind,external_id,title,parent_id) VALUES(?1,?2,?3,?4,?5)
            ON CONFLICT(namespace,kind,external_id) DO UPDATE SET title=excluded.title,parent_id=excluded.parent_id",
            params![media.key.namespace, media.key.kind.as_str(), media.key.id, media.title, parent_id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_media(&self, key: &MediaKey) -> Result<Option<Media>> {
        Ok(self
            .connection
            .query_row(
                &format!("{MEDIA_SELECT} WHERE m.namespace=?1 AND m.kind=?2 AND m.external_id=?3"),
                params![key.namespace, key.kind.as_str(), key.id],
                read_media,
            )
            .optional()?)
    }

    fn list_media(&self, suffix: &str, id: i64, page: Page) -> Result<Vec<Media>> {
        let mut statement = self
            .connection
            .prepare(&format!("{MEDIA_SELECT} {suffix} LIMIT ?2 OFFSET ?3"))?;
        Ok(statement
            .query_map(params![id, page.limit, page.offset], read_media)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn add_to_watchlist(&mut self, key: &MediaKey) -> Result<()> {
        let id = media_id(&self.connection, key)?;
        self.connection.execute(
            "INSERT INTO watchlist(media_id) VALUES(?1) ON CONFLICT(media_id) DO NOTHING",
            [id],
        )?;
        Ok(())
    }

    pub fn remove_from_watchlist(&mut self, key: &MediaKey) -> Result<()> {
        let id = media_id(&self.connection, key)?;
        self.connection
            .execute("DELETE FROM watchlist WHERE media_id=?1", [id])?;
        Ok(())
    }

    pub fn is_in_watchlist(&self, key: &MediaKey) -> Result<bool> {
        let id = media_id(&self.connection, key)?;
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM watchlist WHERE media_id=?1)",
            [id],
            |r| r.get(0),
        )?)
    }

    pub fn list_watchlist(&self, page: Page) -> Result<Vec<Media>> {
        self.list_media(
            "JOIN watchlist w ON w.media_id=m.media_id WHERE ?1=0 ORDER BY w.entry_id DESC",
            0,
            page,
        )
    }

    pub fn list_episodes(&self, series: &MediaKey, page: Page) -> Result<Vec<Media>> {
        ensure!(series.kind == MediaKind::Series, "expected a series key");
        self.list_media(
            "WHERE m.parent_id=?1 ORDER BY m.media_id",
            media_id(&self.connection, series)?,
            page,
        )
    }

    pub fn create_playlist(&mut self, name: &str) -> Result<Playlist> {
        ensure!(!name.trim().is_empty(), "playlist name cannot be empty");
        self.connection
            .execute("INSERT INTO playlists(name) VALUES(?1)", [name])?;
        Ok(Playlist {
            id: self.connection.last_insert_rowid(),
            name: name.into(),
        })
    }

    pub fn rename_playlist(&mut self, id: i64, name: &str) -> Result<()> {
        ensure!(!name.trim().is_empty(), "playlist name cannot be empty");
        ensure!(
            self.connection.execute(
                "UPDATE playlists SET name=?1 WHERE playlist_id=?2",
                params![name, id]
            )? == 1,
            "playlist not found"
        );
        Ok(())
    }

    pub fn delete_playlist(&mut self, id: i64) -> Result<()> {
        self.connection
            .execute("DELETE FROM playlists WHERE playlist_id=?1", [id])?;
        Ok(())
    }

    pub fn list_playlists(&self, page: Page) -> Result<Vec<Playlist>> {
        Ok(self
            .connection
            .prepare(
                "SELECT playlist_id,name FROM playlists ORDER BY playlist_id LIMIT ?1 OFFSET ?2",
            )?
            .query_map(params![page.limit, page.offset], |r| {
                Ok(Playlist {
                    id: r.get(0)?,
                    name: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn add_playlist_item(&mut self, playlist: i64, key: &MediaKey) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        playlist_exists(&tx, playlist)?;
        let id = media_id(&tx, key)?;
        tx.execute(
            "INSERT INTO playlist_items(playlist_id,media_id,position)
            SELECT ?1,?2,COALESCE(MAX(position)+1,0) FROM playlist_items WHERE playlist_id=?1
            ON CONFLICT(playlist_id,media_id) DO NOTHING",
            params![playlist, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_playlist_item(&mut self, playlist: i64, key: &MediaKey) -> Result<()> {
        playlist_exists(&self.connection, playlist)?;
        let id = media_id(&self.connection, key)?;
        self.connection.execute(
            "DELETE FROM playlist_items WHERE playlist_id=?1 AND media_id=?2",
            params![playlist, id],
        )?;
        Ok(())
    }

    /// Replace the order, not membership. Requires each current item exactly once.
    pub fn reorder_playlist(&mut self, playlist: i64, keys: &[MediaKey]) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        playlist_exists(&tx, playlist)?;
        let current = tx
            .prepare("SELECT media_id FROM playlist_items WHERE playlist_id=?1")?
            .query_map([playlist], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        ensure!(
            keys.len() == current.len(),
            "reorder must include every playlist item once"
        );
        let ordered = keys
            .iter()
            .map(|key| media_id(&tx, key))
            .collect::<Result<Vec<_>>>()?;
        let proposed: HashSet<_> = ordered.iter().copied().collect();
        ensure!(
            proposed == current && ordered.len() == proposed.len(),
            "reorder must include every playlist item once"
        );
        tx.execute(
            "DELETE FROM playlist_items WHERE playlist_id=?1",
            [playlist],
        )?;
        for (position, id) in ordered.iter().enumerate() {
            tx.execute(
                "INSERT INTO playlist_items(playlist_id,media_id,position) VALUES(?1,?2,?3)",
                params![playlist, id, position as i64],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_playlist_items(&self, playlist: i64, page: Page) -> Result<Vec<Media>> {
        playlist_exists(&self.connection, playlist)?;
        self.list_media("JOIN playlist_items i ON i.media_id=m.media_id WHERE i.playlist_id=?1 ORDER BY i.position", playlist, page)
    }

    pub fn watched_state(&self, key: &MediaKey) -> Result<WatchedState> {
        let id = media_id(&self.connection, key)?;
        Ok(self
            .connection
            .query_row(
                "SELECT watched,manual_override FROM watched WHERE media_id=?1",
                [id],
                |r| {
                    Ok(WatchedState {
                        watched: r.get(0)?,
                        manual_override: r.get(1)?,
                    })
                },
            )
            .optional()?
            .unwrap_or(WatchedState {
                watched: false,
                manual_override: None,
            }))
    }

    pub fn set_watched(&mut self, key: &MediaKey, watched: bool) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = media_id(&tx, key)?;
        tx.execute("INSERT INTO watched(media_id,watched,manual_override) VALUES(?1,?2,?2)
            ON CONFLICT(media_id) DO UPDATE SET watched=excluded.watched,manual_override=excluded.manual_override", params![id, watched])?;
        if !watched {
            tx.execute("DELETE FROM progress WHERE media_id=?1", [id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn clear_watched_override(&mut self, key: &MediaKey) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = media_id(&tx, key)?;
        tx.execute("INSERT INTO watched(media_id,watched,manual_override)
            VALUES(?1,COALESCE((SELECT position_seconds/duration_seconds>=?2 FROM progress WHERE media_id=?1),0),NULL)
            ON CONFLICT(media_id) DO UPDATE SET watched=excluded.watched,manual_override=NULL", params![id, COMPLETION_THRESHOLD])?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_watched(&self, page: Page) -> Result<Vec<Media>> {
        self.list_media("JOIN watched w ON w.media_id=m.media_id WHERE w.watched=1 AND ?1=0 ORDER BY m.media_id DESC", 0, page)
    }

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
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let id = media_id(&tx, key)?;
        tx.execute("INSERT INTO progress(media_id,position_seconds,duration_seconds) VALUES(?1,?2,?3)
            ON CONFLICT(media_id) DO UPDATE SET position_seconds=excluded.position_seconds,duration_seconds=excluded.duration_seconds", params![id, position, duration_seconds])?;
        tx.execute("INSERT INTO watched(media_id,watched) VALUES(?1,?2)
            ON CONFLICT(media_id) DO UPDATE SET watched=CASE WHEN watched.manual_override IS NOT NULL THEN watched.manual_override ELSE MAX(watched.watched,excluded.watched) END",
            params![id, is_complete(position, duration_seconds)])?;
        tx.commit()?;
        Ok(())
    }

    pub fn load_progress(&self, key: &MediaKey) -> Result<Option<Progress>> {
        let id = media_id(&self.connection, key)?;
        Ok(self
            .connection
            .query_row(
                "SELECT position_seconds,duration_seconds FROM progress WHERE media_id=?1",
                [id],
                |r| {
                    Ok(Progress {
                        position_seconds: r.get(0)?,
                        duration_seconds: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }
}
