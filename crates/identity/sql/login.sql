-- Stores a passkey's new counter, ends the session the browser presented,
-- starts the operator's session, and names its device.
SELECT
    s.outcome,
    s.session_id,
    identity.describe_session($5, $7) AS described
FROM identity.login($1, $2, $3::text::jsonb, $4::text::jsonb, $5, $6) AS s;
