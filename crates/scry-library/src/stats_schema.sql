CREATE TABLE stats_tracking (
    id INTEGER PRIMARY KEY NOT NULL CHECK(id=1),
    started_at BIGINT NOT NULL
);
INSERT INTO stats_tracking VALUES (1, unixepoch());
CREATE TABLE media_runtime (
    media_id INTEGER PRIMARY KEY NOT NULL REFERENCES media(media_id),
    seconds REAL CHECK(seconds>0),
    watched_at BIGINT
);
INSERT INTO media_runtime(media_id,seconds) SELECT media_id,duration_seconds FROM progress;
CREATE TABLE stats_titles (
    media_id INTEGER PRIMARY KEY NOT NULL REFERENCES media(media_id),
    seconds REAL NOT NULL DEFAULT 0 CHECK(seconds>=0),
    rewatch_seconds REAL NOT NULL DEFAULT 0 CHECK(rewatch_seconds>=0),
    completions BIGINT NOT NULL DEFAULT 0 CHECK(completions>=0),
    run_seconds REAL NOT NULL DEFAULT 0 CHECK(run_seconds>=0),
    run_completed BOOLEAN NOT NULL DEFAULT 0 CHECK(run_completed IN (0,1))
);
CREATE TABLE stats_hours (
    hour BIGINT NOT NULL,
    media_id INTEGER NOT NULL REFERENCES media(media_id),
    seconds REAL NOT NULL CHECK(seconds>0),
    PRIMARY KEY(hour, media_id)
);
CREATE TABLE stats_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    started_at BIGINT NOT NULL,
    last_at BIGINT NOT NULL,
    seconds REAL NOT NULL CHECK(seconds>=0),
    episodes BIGINT NOT NULL CHECK(episodes>=0)
);
CREATE TABLE achievement_checks (
    id TEXT PRIMARY KEY NOT NULL,
    unlocked BOOLEAN NOT NULL CHECK(unlocked IN (0,1)),
    historical BOOLEAN NOT NULL CHECK(historical IN (0,1)),
    unlocked_at BIGINT
);
CREATE VIEW stats_counted AS
SELECT m.media_id, COALESCE(t.seconds,0.0) AS recorded_seconds,
    CASE WHEN w.watched=1 THEN MAX(COALESCE(r.seconds,0.0)-COALESCE(t.seconds,0.0),0.0) ELSE 0.0 END AS credit,
    r.watched_at
FROM media m LEFT JOIN stats_titles t USING(media_id)
LEFT JOIN media_runtime r USING(media_id) LEFT JOIN effective_watched w USING(media_id)
WHERE m.kind IN ('movie','episode');
