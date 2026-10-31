-- The status service supplies the same query used for counters and badges.
CREATE VIEW effective_watched AS
SELECT m.media_id,
    CASE WHEN m.kind='series' THEN
            (SELECT complete FROM ({series_status}))
        ELSE COALESCE(w.watched,0) END AS watched
FROM media m LEFT JOIN watched w ON w.media_id=m.media_id;
