-- Removes a session in a realm, saying whether one was there.
SELECT identity.end_session($1, $2) AS ended;
