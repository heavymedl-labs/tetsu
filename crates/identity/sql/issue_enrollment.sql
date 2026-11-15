-- Stores an operator's enrollment link under its token's hash, and says what
-- came of it.
SELECT identity.issue_enrollment($1, $2) AS issued;
