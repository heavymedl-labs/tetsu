-- Stores an operator's enrollment link under its token's hash.
SELECT identity.issue_enrollment($1, $2) AS issued;
