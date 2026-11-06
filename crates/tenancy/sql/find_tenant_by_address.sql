SELECT
    t.id,
    t.slug,
    t.name,
    t.primary_color,
    t.accent_color
FROM tenancy.addresses AS a
INNER JOIN tenancy.tenants AS t ON a.tenant_id = t.id
WHERE a.address = $1;
