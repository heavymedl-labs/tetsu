-- The role a staff member holds at this business, if any.
SELECT r.role_name
FROM identity.staff_roles AS r
WHERE r.staff_id = $1;
