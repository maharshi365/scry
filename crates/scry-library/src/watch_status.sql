-- Shared by the status service and the persisted completion view.
-- Restrict the inventory to one parent before aggregating it.
SELECT counts.*, total > 0 AND watched = total AS complete
FROM (
    SELECT COUNT(*) AS total,
        COALESCE(SUM(CASE WHEN r.released_at <= {now} THEN 1 ELSE 0 END), 0) AS released,
        COALESCE(SUM(CASE WHEN ({eligible}) AND w.watched = 1 THEN 1 ELSE 0 END), 0) AS watched,
        COALESCE(SUM(CASE
            WHEN NOT ({eligible}) THEN 0.0
            WHEN w.watched = 1 THEN 1.0
            ELSE MIN(COALESCE(v.watched_seconds / p.duration_seconds, 0.0), 1.0)
        END), 0.0) AS completed,
        COALESCE(SUM(CASE
            WHEN NOT ({eligible}) THEN 0.0
            WHEN w.watched = 1 THEN 1.0
            ELSE COALESCE(p.position_seconds / p.duration_seconds, 0.0)
        END), 0.0) AS resumed
    FROM media e
    JOIN episode_releases r ON r.media_id = e.media_id
    LEFT JOIN watched w ON w.media_id = e.media_id
    LEFT JOIN progress p ON p.media_id = e.media_id
    LEFT JOIN viewing v ON v.media_id = e.media_id
    WHERE e.parent_id = {parent}
) counts
