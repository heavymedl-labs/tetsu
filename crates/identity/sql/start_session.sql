-- Stores a session under its token's hash, starting now.
SELECT identity.start_session($1, $2, $3);
