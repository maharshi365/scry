# Local library and progress

`scry-library` owns synchronous SQLite access. The app calls it on a dedicated
database worker; add-on configuration uses blocking tasks. No database calls run
on the UI thread or a network executor thread.

## Fresh build

The app now uses **`scry-v2.sqlite3`** in the OS-local application-data folder
(`%LOCALAPPDATA%\Scry\Scry\data` on Windows). This is a new, empty store. Older
databases and add-on settings are not imported. Existing files are left untouched.
You must install your add-ons and save titles again.

There is one embedded schema: `crates/scry-library/src/schema.sql`. There are no
migration files, migration runner, or compatibility layer. `PRAGMA user_version`
identifies the schema, but does not upgrade it. Startup creates the schema only
in an empty database, inside an immediate transaction. A process-local startup
gate also serializes the initial WAL switch, which can otherwise return BUSY
without honoring the busy timeout. Concurrent workers cannot
partly initialize it. Unknown versions, nonempty unversioned stores, missing
mapped columns, and invalid foreign keys are rejected.

For another pre-release schema reset, edit `schema.sql`, update the Diesel types
in `schema.rs`, and use an empty database. Close the app before removing a local
database, including its SQLite sidecars. Do not copy a live WAL database as if the
main file alone were a complete backup.

Installed add-on manifests and URLs are stored in the same database as the
library. URLs can contain credentials. The database is local and unencrypted;
do not share it. Artwork pixels and temporary media remain external files.

## Sources of truth

| Record | Meaning |
| --- | --- |
| `media` | Namespaced movie, show, or episode identity; episode parent |
| `watchlist` | Saved library membership, independent of watch state |
| `progress` | One video's resume position and known duration, in seconds |
| `viewing` | Accumulated actual playback seconds, active resume entry, recency |
| `watched` | Per-video watched state, set by playback or explicit actions |
| `episode_releases` | Dated release metadata, including future episodes and movies (movie rows have no season/episode) |
| `series_inventory` | Whether a show's inventory has been synchronized, even if empty |
| `effective_watched` | A SQL view; derives show completion from episode records |

Show-level automatic watched flags are **not stored separately**. A show is
automatically watched only when it has at least one known released episode and
all known released episodes are watched. There is no independent show override.
Episode changes affect Library filtering and badges immediately;
there is no reconciliation write or network round trip to make them agree.
Known release dates are evaluated locally, so an already-known new release can
change the result without downloading metadata again. The worker returns the
next known release timestamp and the UI schedules a wake to refresh cached
lists and badges when that release becomes eligible.

Marking a show watched marks all its known released episodes, atomically, across
all pages and seasons. Future and undated episodes are not invented as watched.
Marking a show unwatched resets its stored episodes. The resulting show status
is always derived from its episodes. Season actions use their captured full
release inventory in the same transaction, so a pending background refresh
cannot accidentally allow a future episode through.

## Resume, watched time, and show progress are different

- `record_playback(key, position, duration, watched_delta)` saves a video's resume
  sample and adds actual playback time. The UI excludes seeks, pauses, and
  buffering from the delta. More than 70% accumulated playback marks the video
  watched. That automatic result is sticky.
- `save_progress` writes **resume only**. Jumping to 95% cannot mark a video watched.
- Accumulated playback is not unique time-range coverage: replayed time counts.
  Callers must not resend an already-acknowledged delta as new playback.
- `set_watched(false)` clears that video's resume and accumulated playback in
  one transaction. Automatic tracking is immediately available again. There is
  no Auto track button and no permanent false override in the app's actions.
- Marking a known future release watched is a successful no-op, not an error.
  A debug log records the skip without title names or identities. Bulk actions
  continue with eligible episodes. Known future dates also prevent playback
  samples from setting watched status. Missing dates are not classified as future.
  If new metadata identifies a previously undated item as future, its watched
  flag is cleared without deleting its resume history.
- Movie and show actions capture release metadata before mutating state; saved
  titles without cached metadata fetch it in a bounded background task. Superseded
  responses cannot undo a newer watched/unwatched action. Future release years
  provide a lower bound when an exact movie date is missing.
  Already-stored episode inventories can be used directly. Provider failures
  fall back to local release facts rather than making offline watched actions fail.
- Position is finite and clamped to `[0, duration]`. Duration must be finite and
  positive. Shows themselves cannot receive video playback samples.
- `finish_playback` beyond 90% removes the finished active resume entry. It does
  not invent watched time. Advancing preserves any existing next-episode resume
  position and does not reactivate an already-watched next episode.

`SeriesProgress` is an episode-weighted summary over the full stored inventory,
not the visible episode page:

```text
released_episodes       = count of episodes released by the requested time
watched_episodes        = count of those episodes marked watched
completed_equivalents   = watched episodes + sum(partial actual playback / duration)
resume_equivalents      = watched episodes + sum(partial resume position / duration)
show fraction           = completed_equivalents / released_episodes
```

Partial actual-playback fractions are capped at one. An empty inventory has no
fraction. Future, undated, and withdrawn release entries do not inflate the
denominator. Each episode has equal weight; this is not a runtime-weighted
estimate of a show's total duration, because providers often omit durations.

Show posters use the aggregate fraction and show the watched/released episode
count. Currently-watching and next-episode cards use that specific video's
resume progress. A 25%-played episode is not presented as a 25%-completed show.

## Lists and metadata

- `list_library(watched, kind, page)` filters before pagination. To watch contains
  saved, unwatched titles. Watched contains watched titles regardless of membership.
- `list_up_next(now, page)` returns saved unwatched movies and **one earliest
  released, unwatched episode per saved show**. Shows do not occupy hundreds of
  cards. Clicking a card opens the exact episode in the existing season/episode
  picker, where all other episodes remain available.
- Episode order is regular seasons, then specials, then unnumbered seasons;
  ties use episode number and stable database identity. Watched and release
  filters apply before the 200-title page limit.
- `sync_episode_releases` preserves watch history and progress. It writes only
  changed/new release rows and removes withdrawn release entries, not history.
  Provider failures and omitted episode lists leave the previous inventory intact.
- Background metadata checks use 12-show pages and bounded concurrency. Cached
  metadata expires after ten minutes. Showing a poster no longer serializes and
  sends the full released-episode key list on each viewport change.
- Library, currently-watching, next-up, and calendar paging are independent.

## Query and worker boundaries

`media_states(keys)` reads up to 200 identities in one bound query, preserving
input order, duplicates, missing identities, namespaces, watched state,
resume samples, and accumulated playback seconds. Visible movie and episode
state reads do not perform one metadata lookup and several status queries per card.
Resume-list progress is joined into the page query, not loaded in an N+1 loop.
Metadata batches, metadata-plus-playback samples, and season actions use one transaction.

Indexes cover namespaced identity, parent episodes, media kind/order, watchlist
membership, watched status, active/recent viewing, release dates, and playlist
reverse membership. The viewing recency index also avoids scanning all playback
rows to obtain the next ordering value.

Connections use WAL, foreign keys, and a five-second busy timeout. Commands and
events use bounded queues. Generation checks reject stale reads. Read failures
do not consume mutation acknowledgements. The UI keeps its last keyed poster
states during refresh instead of flashing empty status while new reads finish.
Shutdown drains completions and waits for a FIFO write barrier off the UI thread.

The repository also has standalone ordered-playlist APIs. The app does not expose
playlist controls; that existing storage is retained, not expanded by this work.

Run `cargo test -p scry-library` for the storage tests, or
`cargo test --workspace --all-targets` for the full regression suite.
