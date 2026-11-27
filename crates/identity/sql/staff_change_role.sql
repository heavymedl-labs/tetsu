-- Changes a staff member's role at this business from the one it was decided
-- on, and says what came of it.
SELECT identity.staff_change_role($1, $2, $3, $4, $5) AS outcome;
