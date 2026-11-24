-- Takes an addition for the session that began it, once, if it is still open.
SELECT
    t.admin_id,
    t.state
FROM identity.take_addition($1, $2) AS t;
