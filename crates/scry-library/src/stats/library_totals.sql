WITH saved AS MATERIALIZED (
    SELECT m.kind,
        CASE WHEN m.kind = 'series' THEN ({complete})
            ELSE COALESCE(w.watched, 0) END AS watched
    FROM watchlist l JOIN media m USING(media_id)
    LEFT JOIN watched w USING(media_id)
)
SELECT
    movies_watched,episodes_watched,unknown_runtimes,
    (SELECT COUNT(*) FROM series_facts WHERE total>0 AND watched=total
        AND (last_release IS NULL OR last_release<=unixepoch())) AS series_completed,
    (SELECT COUNT(*) FROM saved WHERE kind='series' AND watched=0) AS unfinished_shows,
    (SELECT COUNT(*) FROM saved) AS saved_titles,
    (SELECT COUNT(*) FROM saved WHERE watched=1) AS saved_watched
FROM watch_totals WHERE id=1
