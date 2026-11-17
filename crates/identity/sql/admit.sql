-- Admits an attempt under a rate limit, or the whole seconds it must wait.
SELECT identity.admit($1, $2) AS wait;
