-- This business's staff, the longest here first.
SELECT
    t.staff_id,
    t.name,
    t.email,
    t.role_name,
    t.joined_at
FROM identity.staff_team() AS t;
