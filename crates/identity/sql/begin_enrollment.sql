-- Keeps an enrollment ceremony's state under its token's hash, if the link
-- still allows it.
SELECT identity.begin_enrollment($1, $2, $3, $4::text::jsonb) AS begun;
