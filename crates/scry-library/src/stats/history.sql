WITH history AS (
    SELECT h.media_id, m.kind, h.seconds
    FROM stats_hours h JOIN media m USING(media_id)
    WHERE h.hour >= {start}
    UNION ALL
    SELECT m.media_id, m.kind, r.seconds - COALESCE(t.seconds, 0.0) AS seconds
    FROM media m
    JOIN watched w USING(media_id)
    JOIN media_runtime r USING(media_id)
    LEFT JOIN stats_titles t USING(media_id)
    WHERE m.kind IN ('movie', 'episode') AND w.watched = 1
        AND r.seconds > COALESCE(t.seconds, 0.0) AND {credit_filter}
)
