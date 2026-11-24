-- The passkey a credential ID names, with whether its admin is disabled.
SELECT
    s.admin_id,
    s.user_handle,
    s.passkey,
    s.disabled
FROM identity.signer($1) AS s;
