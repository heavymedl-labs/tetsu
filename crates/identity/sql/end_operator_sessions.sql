-- Ends every session one operator holds in a realm.
SELECT identity.end_sessions($2, $1) AS ended;
