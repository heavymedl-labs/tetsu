-- Records a request on a live session and returns it, or removes a spent one.
SELECT
    s.realm,
    s.operator_id,
    s.started_at,
    s.proven_at,
    s.last_seen_at,
    s.ends_at
FROM identity.use_session($1, $2) AS s;
