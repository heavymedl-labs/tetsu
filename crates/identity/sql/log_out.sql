-- Removes a session in a realm: whether it was live when it ended, its public
-- id, and its account, or no row for none.
SELECT
    l.live,
    l.session_id::text AS session_id,
    l.account_id
FROM identity.log_out($1, $2) AS l;
