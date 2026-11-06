INSERT INTO tenancy.tenants (slug, name, primary_color, accent_color)
VALUES ($1, $2, $3, $4)
RETURNING id;
