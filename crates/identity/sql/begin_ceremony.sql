-- Keeps a passkey ceremony's state under its token's hash.
SELECT identity.begin_ceremony($1, $2, $3, $4, $5::text::jsonb);
