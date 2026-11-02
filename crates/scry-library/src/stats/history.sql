WITH history AS (
    SELECT media_id,kind,recorded_seconds+credit AS seconds
    FROM watch_facts WHERE {start}=0 AND recorded_seconds+credit>0
    UNION ALL
    SELECT h.media_id, m.kind, h.seconds
    FROM stats_hours h JOIN media m USING(media_id)
    WHERE {start}!=0 AND h.hour >= {start}
    UNION ALL
    SELECT media_id,kind,credit AS seconds FROM watch_facts
    WHERE {start}!=0 AND watched_at>={start} AND credit>0
)
