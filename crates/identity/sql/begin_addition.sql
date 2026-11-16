-- Keeps a ceremony for adding a passkey, if the session still allows it.
SELECT identity.begin_addition($1, $2, $3, $4::text::jsonb) AS begun;
