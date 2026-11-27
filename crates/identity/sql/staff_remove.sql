-- Removes a staff member from this business from the role it was decided on,
-- and says what came of it.
SELECT identity.staff_remove($1, $2, $3, $4) AS outcome;
