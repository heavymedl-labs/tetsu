-- The account a live operator session names, a row for each passkey.
SELECT
    a.operator_id,
    a.user_handle,
    a.name,
    a.credential_id,
    a.transports,
    a.backup_eligible,
    a.backup_state
FROM identity.account($1) AS a;
