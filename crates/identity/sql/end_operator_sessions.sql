-- Ends every session one operator holds in a realm.
SELECT identity.end_operator_sessions($1, $2) AS ended;
