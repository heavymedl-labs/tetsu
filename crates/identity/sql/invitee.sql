-- The admin an open invite link names, if they may join.
SELECT
    e.admin_id AS account_id,
    e.user_handle,
    e.name
FROM identity.invitee($1) AS e;
