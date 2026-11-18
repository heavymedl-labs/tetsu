-- Adds an operator with no passkey, and their email address if given, and
-- says whether it did: a name already
-- taken adds none, and names the operator who holds it, if the statement can
-- see them yet.
WITH added AS (
    INSERT INTO identity.operators (name, email) VALUES ($1, $2)
    ON CONFLICT (name) DO NOTHING
    RETURNING id
)

SELECT
    added.id,
    true AS created
FROM added
UNION ALL
SELECT
    o.id,
    false AS created
FROM identity.operators AS o
WHERE o.name = $1 AND NOT EXISTS (SELECT FROM added);
