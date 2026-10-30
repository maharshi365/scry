CREATE TABLE skip_segments (
    provider TEXT NOT NULL,
    imdb_id TEXT NOT NULL,
    season INTEGER NOT NULL CHECK (season >= 0),
    episode INTEGER NOT NULL CHECK (episode > 0),
    payload TEXT NOT NULL,
    etag TEXT,
    fetched_at INTEGER NOT NULL CHECK (fetched_at >= 0),
    PRIMARY KEY (provider, imdb_id, season, episode)
);
