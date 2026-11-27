-- This business's open invites, the oldest first.
SELECT
    i.invite_id,
    i.staff_id,
    i.email,
    i.role_name,
    i.ends_at
FROM identity.staff_open_invites() AS i;
