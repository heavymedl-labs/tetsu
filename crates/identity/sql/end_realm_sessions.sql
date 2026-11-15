-- Ends every session in a realm.
SELECT identity.end_sessions($1, null) AS ended;
