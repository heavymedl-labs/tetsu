-- Accepts an open invite for the staff member whose fresh session a token's
-- hash names, in the invite's business, and says what came of it.
SELECT identity.staff_accept_invite($1, $2) AS outcome;
