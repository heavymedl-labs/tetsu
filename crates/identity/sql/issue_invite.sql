-- Stores an operator's invite link under its token's hash, and says what
-- came of it.
SELECT identity.issue_invite($1, $2) AS issued;
