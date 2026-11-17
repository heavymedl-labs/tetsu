-- The operator an open invite link names, if they may join.
SELECT
    e.operator_id,
    e.user_handle,
    e.name
FROM identity.invitee($1) AS e;
