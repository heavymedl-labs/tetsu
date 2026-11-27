-- Invites someone to this business from its Team page, and says what came of
-- it.
SELECT
    i.outcome,
    i.invited,
    i.created,
    i.renamed
FROM identity.staff_invite_from($1, $2, $3, $4, $5, $6, $7) AS i;
