-- What an open staff invite link gives, before anyone acts on it: its
-- business and role, and whether its person has a passkey already.
SELECT
    e.tenant_id,
    e.role_name,
    e.joined
FROM identity.staff_invitee($1) AS e;
