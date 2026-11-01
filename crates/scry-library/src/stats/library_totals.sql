WITH states AS MATERIALIZED (
    SELECT m.kind, l.media_id AS saved_id, r.seconds,
        CASE WHEN m.kind = 'series' THEN ({complete})
            ELSE COALESCE(w.watched, 0) END AS watched
    FROM media m
    LEFT JOIN watched w USING(media_id)
    LEFT JOIN watchlist l USING(media_id)
    LEFT JOIN media_runtime r USING(media_id)
)
SELECT
    COALESCE(SUM(kind = 'movie' AND watched = 1), 0) AS movies_watched,
    COALESCE(SUM(kind = 'episode' AND watched = 1), 0) AS episodes_watched,
    COALESCE(SUM(kind = 'series' AND watched = 1), 0) AS series_completed,
    COALESCE(SUM(kind = 'series' AND saved_id IS NOT NULL AND watched = 0), 0) AS unfinished_shows,
    COUNT(saved_id) AS saved_titles,
    COALESCE(SUM(saved_id IS NOT NULL AND watched = 1), 0) AS saved_watched,
    COALESCE(SUM(kind IN ('movie', 'episode') AND watched = 1 AND seconds IS NULL), 0) AS unknown_runtimes
FROM states
