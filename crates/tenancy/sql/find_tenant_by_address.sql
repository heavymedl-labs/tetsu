SELECT
    id,
    slug,
    name,
    primary_color,
    accent_color
FROM tenancy.tenant_at($1);
