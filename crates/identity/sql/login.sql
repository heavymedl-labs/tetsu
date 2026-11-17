-- Stores a passkey's new counter, ends the session the browser presented, and
-- starts the operator's session.
SELECT
    s.outcome,
    s.session_id
FROM identity.login($1, $2, $3::text::jsonb, $4::text::jsonb, $5, $6) AS s;
