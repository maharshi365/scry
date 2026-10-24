# Local library

`crates/scry-library/src/lib.rs` provides synchronous SQLite storage. Run **all** calls, including
open and reads, on a dedicated database worker or `tokio::task::spawn_blocking`.
Do not hold a UI lock across a database call. Keep a long-lived `Library` on the
worker and send it commands. The caller selects a database path in local app data
and creates its parent directory. No network, stream URLs, or addon secrets are
stored. Titles, identity keys, and playlist names are the only text fields.

## Public API

All fallible calls return `anyhow::Result<T>`. Writes use `&mut self`; reads use
`&self`. The module must be exported as `pub mod library;` by the integration.

- `Library::open(impl AsRef<Path>) -> Result<Library>`; `Library::memory()` for tests.
- `MediaKind::{Movie, Series, Episode}`.
- `MediaKey { namespace: String, kind: MediaKind, id: String }`;
  `MediaKey::new(namespace, kind, id)`. Use a stable provider namespace, not its
  display name. The full tuple is the identity, so providers and kinds cannot collide.
- `Media { key: MediaKey, title: String, parent: Option<MediaKey> }`.
  Parent is optional and only valid for an episode pointing to an existing series.
- `upsert_media(&Media) -> Result<()>`, `get_media(&MediaKey) -> Result<Option<Media>>`.
  Upsert replaces title and parent; provide the parent again when refreshing an episode.
  Upsert metadata before adding membership, status, or progress.
- `Page::new(limit: u32, offset: u32) -> Result<Page>`; default is 50 at offset 0.
  Limits must be 1–200 (`MAX_PAGE_SIZE`). Every list takes a page. Offset pagination
  is deterministic on unchanged data, not a cross-request snapshot.
- `add_to_watchlist`, `remove_from_watchlist`: `(&MediaKey) -> Result<()>`.
  `is_in_watchlist(&MediaKey) -> Result<bool>`.
  `list_watchlist(Page) -> Result<Vec<Media>>` returns newest additions first.
- `Playlist { id: i64, name: String }`.
  `create_playlist(&str) -> Result<Playlist>`, `rename_playlist(i64, &str)`,
  `delete_playlist(i64)` return `Result<()>` for the latter two.
  `list_playlists(Page) -> Result<Vec<Playlist>>` uses creation order.
  Names must not be blank; duplicate names are allowed, IDs identify playlists.
- `add_playlist_item(i64, &MediaKey)`, `remove_playlist_item(i64, &MediaKey)`
  return `Result<()>`. Add appends; repeated add is a no-op. Removing an absent
  member is a no-op. Membership has unique positions; removal may leave gaps.
  `reorder_playlist(i64, &[MediaKey]) -> Result<()>` requires exactly the current
  membership, once each. It validates and replaces order in one transaction.
  `list_playlist_items(i64, Page) -> Result<Vec<Media>>` returns explicit order.
- `WatchedState { watched: bool, manual_override: Option<bool> }`.
  `watched_state(&MediaKey) -> Result<WatchedState>`,
  `set_watched(&MediaKey, bool) -> Result<()>`,
  `clear_watched_override(&MediaKey) -> Result<()>`,
  `list_watched(Page) -> Result<Vec<Media>>` (newest metadata insertion first).
- `Progress { position_seconds: f64, duration_seconds: f64 }`.
  `save_progress(&MediaKey, position_seconds: f64, duration_seconds: f64) -> Result<()>`,
  `load_progress(&MediaKey) -> Result<Option<Progress>>`.
- `list_episodes(&MediaKey, Page) -> Result<Vec<Media>>` requires a series and
  returns episode metadata insertion order, not season/episode sorting.
- `is_complete(position_seconds: f64, duration_seconds: f64) -> bool` and
  `COMPLETION_THRESHOLD` (0.9).

Data models derive serde serialization/deserialization. `Page` is validated at
construction. Unknown media keys are errors for state/membership operations;
`get_media` alone returns `None`. Missing playlists are errors except delete,
which is idempotent. Watchlist removal and playlist add are idempotent.

## Playback semantics

Position must be finite and is clamped to `[0, duration]`. Duration must be finite
and strictly positive. Invalid input leaves stored progress and watched state
unchanged. Native playback uses `record_playback(key, position, duration,
watched_delta)`. It stores accumulated playback time separately from position.
More than **70% actual playback time** marks the item watched. Seek events and
position jumps do not add watched time. Automatic watched status is sticky.
The older standalone `save_progress` API still has its original position-based
behaviour; it is not used by the app's playback path.

`finish_playback(key, next)` reconciles resume state when playback stops beyond
90% of its position. It hides the finished item from `list_currently_watching`
and optionally registers the next released episode. The UI supplies that episode
from dated metadata captured when playback starts. There is one active resume
entry per series, ordered by most recent save. Unknown/future episodes are not
selected. Natural EOF clears resume state, but does not invent watched time.

`list_library(watched, kind, page)` filters Movies or TV series before pagination.
The native UI has no playlist controls; existing standalone playlist storage
APIs are not part of the application workflow.

An explicit `set_watched(true)` overrides later progress saves.
`set_watched(false)` clears progress and the finished marker in one transaction,
and installs a false override. Further playback does not mark it watched until
the user clears the override or explicitly marks it watched. Clearing the
override recalculates watched status from accumulated playback time (or false if absent).
Watchlist membership is independent and is never removed automatically.

Series and episode state are independent. Finishing one episode never marks its
series watched. Marking a series watched never cascades to episodes. The UI can
page through episode records to display individual progress; the repository does
not infer series completeness from a partial episode catalog.

## Storage and concurrency

The app stores installed add-ons and library state together in `scry.sqlite3` in
the OS-local application-data folder. Add-on order and full manifests are retained.
Replacing the add-on list is atomic. Images and temporary media remain external
files; they are not app state. Add-on URLs may contain credentials, so the
unencrypted database must not be shared.

Diesel provides typed queries and row mapping. Two correlated media queries use
bound raw SQL where a self-join is clearer. Connections use WAL, foreign keys,
and a five-second busy timeout. Multi-statement writes use immediate transactions.
Database access runs on the library worker or a blocking task, not the UI or
network executor threads. SQLite in-memory databases use the memory journal.

`diesel_migrations` embeds the SQL files in `crates/scry-library/migrations/`.
Opening a database applies pending migrations inside an immediate transaction,
which serializes concurrent startup. Diesel tracks applied versions in
`__diesel_schema_migrations`. Unknown versions, missing mapped tables/columns,
invalid foreign keys, and nonempty unversioned databases are rejected. Do not
open arbitrary third-party databases as app databases.

This is a fresh pre-release schema. The old `library.sqlite3`, `addons.json`, and
their contents are not imported or deleted. There is no legacy compatibility path.

### Changing the schema

1. Add a new versioned directory under `crates/scry-library/migrations/`, with
   `up.sql` and `down.sql`. Do not edit migrations already used by deployed apps.
2. Update `crates/scry-library/src/schema.rs` and affected record mappings.
   SQLite identity and ordering columns use Rust `i64` / Diesel `BigInt`.
3. Add behavior tests for the change, including upgrades that retain existing
   state where applicable. Run `cargo test -p scry-library`.

The build script tracks migration-file changes. The app runs embedded migrations
on startup; end users do not need the Diesel CLI or loose SQL files.

Run `cargo test -p scry-library`. The integration tests use the crate's public API
and do not build the app, UI, or torrent engine.
