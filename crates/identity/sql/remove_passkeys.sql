-- Removes every passkey an operator holds, ending their sessions and
-- ceremonies.
SELECT
    r.passkeys_removed,
    r.sessions_ended
FROM identity.remove_passkeys($1) AS r;
