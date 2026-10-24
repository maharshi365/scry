CREATE TABLE episode_releases (
    media_id INTEGER PRIMARY KEY NOT NULL REFERENCES media(media_id) ON DELETE CASCADE,
    released_at BIGINT NOT NULL,
    season BIGINT,
    episode BIGINT
);
CREATE INDEX episode_releases_date ON episode_releases(released_at);
