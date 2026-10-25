CREATE TABLE media (
    media_id INTEGER PRIMARY KEY NOT NULL,
    namespace TEXT NOT NULL CHECK(length(trim(namespace)) > 0),
    kind TEXT NOT NULL CHECK(kind IN ('movie','series','episode')),
    external_id TEXT NOT NULL CHECK(length(trim(external_id)) > 0),
    title TEXT NOT NULL CHECK(length(trim(title)) > 0),
    parent_id INTEGER REFERENCES media(media_id),
    CHECK(parent_id IS NULL OR kind = 'episode'),
    UNIQUE(namespace, kind, external_id)
);
CREATE INDEX media_parent ON media(parent_id, media_id);
CREATE INDEX media_kind ON media(kind, media_id DESC);
CREATE TABLE watchlist (
    entry_id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    media_id INTEGER NOT NULL UNIQUE REFERENCES media(media_id) ON DELETE CASCADE
);
CREATE TABLE playlists (
    playlist_id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0)
);
CREATE TABLE playlist_items (
    playlist_id INTEGER NOT NULL REFERENCES playlists(playlist_id) ON DELETE CASCADE,
    media_id INTEGER NOT NULL REFERENCES media(media_id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK(position >= 0),
    PRIMARY KEY(playlist_id, media_id),
    UNIQUE(playlist_id, position)
);
CREATE INDEX playlist_items_media ON playlist_items(media_id);
CREATE TABLE progress (
    media_id INTEGER PRIMARY KEY NOT NULL REFERENCES media(media_id) ON DELETE CASCADE,
    position_seconds REAL NOT NULL CHECK(position_seconds >= 0 AND position_seconds <= duration_seconds),
    duration_seconds REAL NOT NULL CHECK(duration_seconds > 0)
);
CREATE TABLE watched (
    media_id INTEGER PRIMARY KEY NOT NULL REFERENCES media(media_id) ON DELETE CASCADE,
    watched BOOLEAN NOT NULL CHECK(watched IN (0,1))
);
CREATE INDEX watched_status ON watched(watched, media_id);
CREATE TABLE viewing (
    media_id INTEGER PRIMARY KEY NOT NULL REFERENCES media(media_id) ON DELETE CASCADE,
    watched_seconds REAL NOT NULL DEFAULT 0 CHECK(watched_seconds >= 0),
    active BOOLEAN NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
    touched INTEGER NOT NULL
);
CREATE INDEX viewing_touched ON viewing(touched DESC);
CREATE INDEX viewing_active ON viewing(active, touched DESC, media_id);
CREATE TABLE addons (
    id TEXT PRIMARY KEY NOT NULL CHECK(length(trim(id)) > 0),
    position INTEGER NOT NULL UNIQUE CHECK(position >= 0),
    manifest_url TEXT NOT NULL CHECK(length(trim(manifest_url)) > 0),
    manifest_json TEXT NOT NULL CHECK(json_valid(manifest_json))
);
CREATE TABLE series_inventory (
    media_id INTEGER PRIMARY KEY NOT NULL REFERENCES media(media_id) ON DELETE CASCADE
);
CREATE TABLE episode_releases (
    media_id INTEGER PRIMARY KEY NOT NULL REFERENCES media(media_id) ON DELETE CASCADE,
    released_at BIGINT,
    season BIGINT CHECK(season IS NULL OR season >= 0),
    episode BIGINT CHECK(episode IS NULL OR episode >= 0)
);
CREATE INDEX episode_releases_date ON episode_releases(released_at, media_id);
