-- Sets an admin's email address (ADR-0074), and says whether it changed,
-- or nothing for an admin who does not exist.
WITH existing AS (
    SELECT
        admins.id,
        admins.email
    FROM identity.admins AS admins
    WHERE admins.id = $1
    FOR NO KEY UPDATE
),

updated AS (
    UPDATE identity.admins AS admins
    SET email = $2
    FROM existing
    WHERE admins.id = existing.id
    RETURNING existing.email IS DISTINCT FROM $2 AS done
)

SELECT done FROM updated;
