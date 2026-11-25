-- Takes a passkey ceremony, once, if it is still open.
SELECT
    c.admin_id AS account_id,
    c.link_hash,
    c.state
FROM identity.take_ceremony($1, $2) AS c;
