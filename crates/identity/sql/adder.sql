-- The admin a fresh session names, if they may add a passkey, a row for each
-- passkey they hold.
SELECT
    a.admin_id AS account_id,
    a.user_handle,
    a.name,
    a.email,
    a.credential_id,
    a.transports,
    a.backup_eligible,
    a.backup_state
FROM identity.adder($1) AS a;
