-- Completion uses the full known inventory, not just released episodes.
CREATE VIEW effective_watched AS
SELECT m.media_id,
    CASE WHEN m.kind='series' THEN
            EXISTS(SELECT 1 FROM media e JOIN episode_releases r ON r.media_id=e.media_id
                WHERE e.parent_id=m.media_id)
            AND NOT EXISTS(SELECT 1 FROM media e JOIN episode_releases r ON r.media_id=e.media_id
                LEFT JOIN watched v ON v.media_id=e.media_id
                WHERE e.parent_id=m.media_id AND
                    (r.released_at IS NULL OR r.released_at>unixepoch() OR COALESCE(v.watched,0)=0))
        ELSE COALESCE(w.watched,0) END AS watched
FROM media m LEFT JOIN watched w ON w.media_id=m.media_id;
