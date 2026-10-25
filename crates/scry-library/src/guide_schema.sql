CREATE TABLE guide_state (
    id INTEGER PRIMARY KEY NOT NULL CHECK(id=1),
    payload TEXT NOT NULL CHECK(json_valid(payload))
);
CREATE TABLE guide_metadata (
    media_id INTEGER PRIMARY KEY NOT NULL REFERENCES media(media_id) ON DELETE CASCADE,
    payload TEXT NOT NULL CHECK(json_valid(payload))
);
