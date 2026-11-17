-- Disables an operator, ending their sessions and invite link.
SELECT
    d.done,
    d.sessions_ended,
    d.links_ended
FROM identity.disable_operator($1) AS d;
