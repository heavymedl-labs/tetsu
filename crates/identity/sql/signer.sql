-- The passkey a credential ID names, with whether its operator is disabled.
SELECT
    s.operator_id,
    s.user_handle,
    s.passkey,
    s.disabled
FROM identity.signer($1) AS s;
