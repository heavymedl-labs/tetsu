-- The operator a fresh session names, if they may add a passkey.
SELECT
    a.operator_id,
    a.user_handle,
    a.name,
    a.credentials
FROM identity.adder($1) AS a;
