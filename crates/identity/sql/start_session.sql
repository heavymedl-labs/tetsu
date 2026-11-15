-- Stores a session under its token's hash.
SELECT identity.start_session($1, $2, $3, $4, $5);
