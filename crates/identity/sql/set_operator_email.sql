-- Sets an operator's email address (ADR-0074), and says whether it changed,
-- or nothing for an operator who does not exist.
WITH existing AS (
    SELECT
        operators.id,
        operators.email
    FROM identity.operators AS operators
    WHERE operators.id = $1
    FOR NO KEY UPDATE
),

updated AS (
    UPDATE identity.operators AS operators
    SET email = $2
    FROM existing
    WHERE operators.id = existing.id
    RETURNING existing.email IS DISTINCT FROM $2 AS done
)

SELECT done FROM updated;
