-- The staff member an open invite link names, if they may join by it: one
-- with a passkey already accepts it instead, after logging in.
SELECT
    e.staff_id AS account_id,
    e.user_handle,
    e.name
FROM identity.staff_invitee($1) AS e
WHERE NOT e.joined;
