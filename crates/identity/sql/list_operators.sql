-- Every operator, oldest first.
SELECT
    o.id,
    o.name,
    o.created_at,
    o.disabled_at
FROM identity.operators AS o
ORDER BY o.id;
