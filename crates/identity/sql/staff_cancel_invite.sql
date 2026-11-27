-- Cancels an open invite at this business by its random id, and says what
-- came of it.
SELECT identity.staff_cancel_invite($1, $2, $3, $4) AS outcome;
