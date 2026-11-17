-- Keeps a join ceremony's state under its token's hash, if the link
-- still allows it.
SELECT identity.begin_join($1, $2, $3, $4::text::jsonb) AS begun;
