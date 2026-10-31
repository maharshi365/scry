CREATE TABLE saved_streams (
    media_id INTEGER PRIMARY KEY REFERENCES media(media_id) ON DELETE CASCADE,
    stream_json TEXT NOT NULL CHECK(length(stream_json) <= 65536)
);
