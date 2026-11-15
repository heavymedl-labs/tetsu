-- Ends every session in a realm.
SELECT identity.end_realm_sessions($1) AS ended;
