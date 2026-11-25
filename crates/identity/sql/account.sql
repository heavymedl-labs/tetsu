-- The account a live admin session names, a row for each passkey.
SELECT
    a.admin_id AS account_id,
    a.user_handle,
    a.name,
    a.email,
    a.credential_id,
    a.transports,
    a.backup_eligible,
    a.backup_state
FROM identity.account($1) AS a;
