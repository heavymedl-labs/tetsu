-- Stores an admin's added passkey, using up its addition and ending their
-- other sessions.
SELECT
    a.added,
    a.sessions_ended,
    a.session_id
FROM identity.add_passkey($1, $2, $3, $4, $5::text::jsonb, $6) AS a;
