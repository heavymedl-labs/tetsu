-- Gives a tenant a new slug and the address that goes with it, for a tenant
-- whose address a release has since reserved for Tetsu's own pages (#90).
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- Only the slug and the address change, and only through the function below.
GRANT UPDATE (slug) ON tenants TO tetsu_tenancy_lookup;
GRANT UPDATE (address) ON addresses TO tetsu_tenancy_lookup;

-- Moves the tenant at one address to a new slug and its path, and says
-- whether a tenant was there. A slug or address another tenant holds is
-- refused by its unique key, so nothing changes.
CREATE FUNCTION rename_tenant(wanted_address text, new_slug text)
RETURNS boolean
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH moved AS (
        UPDATE tenancy.addresses AS a
        SET address = '/' || new_slug
        WHERE a.address = wanted_address
        RETURNING a.tenant_id
    ),

    renamed AS (
        UPDATE tenancy.tenants AS t
        SET slug = new_slug
        FROM moved
        WHERE t.id = moved.tenant_id
        RETURNING t.id
    )

    SELECT EXISTS (SELECT FROM renamed)
$$;

REVOKE EXECUTE ON FUNCTION rename_tenant(text, text) FROM public;
GRANT EXECUTE ON FUNCTION rename_tenant(text, text) TO tetsu_tenancy;

GRANT CREATE ON SCHEMA tenancy TO tetsu_tenancy_lookup;
ALTER FUNCTION rename_tenant(text, text) OWNER TO tetsu_tenancy_lookup;
REVOKE CREATE ON SCHEMA tenancy FROM tetsu_tenancy_lookup;
