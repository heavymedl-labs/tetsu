-- Whole seconds to each limit on a live session, read without renewing it.
SELECT
    s.idle_left,
    s.absolute_left
FROM identity.session_left($1, $2) AS s;
