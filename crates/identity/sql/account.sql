-- The account a live operator session names.
SELECT
    a.operator_id,
    a.user_handle,
    a.name,
    a.credentials
FROM identity.account($1) AS a;
