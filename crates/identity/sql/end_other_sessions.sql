-- Ends one of the admin's other sessions, by its random id, or every other.
SELECT
    e.who,
    e.asking,
    e.ended,
    e.reason
FROM identity.end_other_sessions($1, $2::text::uuid) AS e;
