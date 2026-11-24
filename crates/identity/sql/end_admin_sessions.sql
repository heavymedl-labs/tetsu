-- Ends every session one admin holds in a realm.
SELECT identity.end_sessions($2, $1) AS ended;
