-- Keeps a login ceremony's state under its token's hash.
SELECT identity.begin_login($1, $2::text::jsonb);
