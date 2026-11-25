-- The business an open invite link names, so redeeming it runs in that
-- business's own transaction.
SELECT e.tenant_id FROM identity.staff_invitee($1) AS e;
