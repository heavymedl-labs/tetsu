-- Invites the staff member an email names to this business with a role,
-- under a link's hash, and says what came of it.
SELECT
    i.outcome,
    i.invited,
    i.created,
    i.renamed
FROM identity.staff_issue_invite($1, $2, $3, $4) AS i;
