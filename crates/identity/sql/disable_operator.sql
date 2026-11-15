-- Disables an operator, ending their sessions and enrollment link.
SELECT identity.disable_operator($1) AS done;
