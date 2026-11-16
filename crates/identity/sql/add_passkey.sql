-- Stores an operator's added passkey, ending their other sessions.
SELECT
    a.added,
    a.sessions_ended
FROM identity.add_passkey($1, $2, $3, $4::text::jsonb) AS a;
