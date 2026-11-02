CREATE TABLE IF NOT EXISTS dirty_titles (media_id INTEGER PRIMARY KEY NOT NULL);
CREATE TABLE IF NOT EXISTS dirty_series (media_id INTEGER PRIMARY KEY NOT NULL);
CREATE TABLE IF NOT EXISTS source_revisions (
    id INTEGER PRIMARY KEY NOT NULL CHECK(id=1),
    library BIGINT NOT NULL DEFAULT 0,
    playback BIGINT NOT NULL DEFAULT 0,
    hours BIGINT NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO source_revisions(id) VALUES(1);
CREATE TABLE IF NOT EXISTS watch_facts (
    media_id INTEGER PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('movie','episode')),
    watched BOOLEAN NOT NULL,
    recorded_seconds REAL NOT NULL,
    credit REAL NOT NULL,
    watched_at BIGINT,
    unknown_runtime BOOLEAN NOT NULL
);
CREATE INDEX IF NOT EXISTS watch_facts_date ON watch_facts(watched_at) WHERE credit>0;
CREATE TABLE IF NOT EXISTS watch_totals (
    id INTEGER PRIMARY KEY NOT NULL CHECK(id=1),
    movie_seconds REAL NOT NULL DEFAULT 0,
    tv_seconds REAL NOT NULL DEFAULT 0,
    movies_watched BIGINT NOT NULL DEFAULT 0,
    episodes_watched BIGINT NOT NULL DEFAULT 0,
    unknown_runtimes BIGINT NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO watch_totals(id) VALUES(1);
CREATE TABLE IF NOT EXISTS series_facts (
    media_id INTEGER PRIMARY KEY NOT NULL,
    total BIGINT NOT NULL,
    watched BIGINT NOT NULL,
    last_release BIGINT
);
CREATE TABLE IF NOT EXISTS stats_days (
    day BIGINT PRIMARY KEY NOT NULL,
    seconds REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS stats_hours_media ON stats_hours(media_id,hour);
CREATE INDEX IF NOT EXISTS stats_hours_day ON stats_hours(hour/86400,media_id);

DROP VIEW stats_counted;
CREATE VIEW stats_counted AS
SELECT media_id,recorded_seconds,credit,watched_at FROM watch_facts;
DROP VIEW effective_watched;
CREATE VIEW effective_watched AS
SELECT m.media_id,COALESCE(w.watched,0) AS watched
FROM media m LEFT JOIN watched w USING(media_id) WHERE m.kind!='series'
UNION ALL
SELECT m.media_id,COALESCE(s.total>0 AND s.watched=s.total
    AND (s.last_release IS NULL OR s.last_release<=unixepoch()),0) AS watched
FROM media m LEFT JOIN series_facts s USING(media_id) WHERE m.kind='series';
