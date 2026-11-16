-- The passkey a credential ID names, if its operator may sign in.
SELECT
    s.operator_id,
    s.user_handle,
    s.passkey
FROM identity.signer($1) AS s;
