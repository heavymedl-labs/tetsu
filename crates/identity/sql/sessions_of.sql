-- The live sessions of whoever's live session this is, by their random ids.
SELECT
    s.id,
    s.started_at,
    s.last_seen_at,
    s.device,
    s.presented
FROM identity.sessions_of($1) AS s;
