DROP VIEW effective_watched;
CREATE TABLE episode_releases_v2 (
    media_id INTEGER PRIMARY KEY NOT NULL REFERENCES media(media_id) ON DELETE CASCADE,
    released_at BIGINT,
    season BIGINT CHECK(season IS NULL OR season >= 0),
    episode BIGINT CHECK(episode IS NULL OR episode >= 0)
);
INSERT INTO episode_releases_v2 SELECT media_id, released_at, season, episode FROM episode_releases;
DROP TABLE episode_releases;
ALTER TABLE episode_releases_v2 RENAME TO episode_releases;
CREATE INDEX episode_releases_date ON episode_releases(released_at, media_id);
