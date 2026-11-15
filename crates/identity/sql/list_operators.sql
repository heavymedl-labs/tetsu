-- Every operator, oldest first, with their passkeys and open link.
SELECT
    o.id,
    o.name,
    o.created_at,
    o.disabled_at,
    o.passkeys,
    o.link_minutes
FROM identity.list_operators() AS o;
