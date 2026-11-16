-- Stores a passkey's new counter and starts the operator's session.
SELECT
    s.signed,
    s.session_id
FROM identity.sign_in($1, $2, $3::text::jsonb, $4::text::jsonb, $5) AS s;
