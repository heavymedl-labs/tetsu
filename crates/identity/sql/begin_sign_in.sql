-- Keeps a sign-in ceremony's state under its token's hash.
SELECT identity.begin_sign_in($1, $2::text::jsonb);
