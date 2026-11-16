-- Takes a ceremony for adding a passkey, once, if it is still open.
SELECT
    c.operator_id,
    c.state
FROM identity.take_ceremony($1, 'add') AS c;
