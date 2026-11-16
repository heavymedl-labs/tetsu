-- Stores a passkey's new counter and starts the operator's session.
SELECT identity.sign_in($1, $2::text::jsonb, $3::text::jsonb, $4) AS signed;
