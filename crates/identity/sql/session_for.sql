-- The one session a token's hash names, if any.
SELECT
    s.realm,
    s.operator_id,
    s.started_at,
    s.last_seen_at,
    s.ends_at
FROM identity.session_for($1, $2) AS s;
