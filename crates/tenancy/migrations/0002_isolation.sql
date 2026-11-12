-- Tenancy under ADR-0042: each table declares its class, locations are
-- tenant-owned, and lookups that run before a tenant is known go through
-- narrow functions owned by the lookup role.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

COMMENT ON TABLE tenants IS 'tetsu:global: the registry that issues tenant_id';
COMMENT ON TABLE addresses IS 'tetsu:global: read before a tenant is known';
COMMENT ON TABLE locations IS 'tetsu:tenant-owned';

ALTER TABLE locations ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant ON locations TO tetsu_tenancy
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint)
);

CREATE POLICY tenant_only ON locations AS RESTRICTIVE TO tetsu_tenancy
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint)
);

-- Rights given before this file: the application role's directly, as the
-- demo's still holds them, and the module role's from the runner's default
-- privileges on a new database. Each is replaced by exactly what is granted
-- below.
REVOKE ALL ON tenants, addresses, locations, migrations FROM tetsu_app;
REVOKE ALL ON SEQUENCE tenants_id_seq FROM tetsu_app;
REVOKE ALL ON SCHEMA tenancy FROM tetsu_app;
REVOKE ALL ON tenants, addresses, locations FROM tetsu_tenancy;
REVOKE ALL ON SEQUENCE tenants_id_seq FROM tetsu_tenancy;

-- Tenancy's own role writes and reads locations, and reaches tenants and
-- addresses only through the functions below, so no caller can list or
-- rewrite them. Those run as the lookup role.
GRANT SELECT, INSERT ON locations TO tetsu_tenancy;
GRANT SELECT, INSERT ON tenants, addresses TO tetsu_tenancy_lookup;

-- Registers a tenant at its first address, and returns its id.
CREATE FUNCTION register_tenant(
    wanted_slug text,
    wanted_name text,
    wanted_primary text,
    wanted_accent text,
    wanted_address text
)
RETURNS bigint
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH registered AS (
        INSERT INTO tenancy.tenants (slug, name, primary_color, accent_color)
        VALUES (wanted_slug, wanted_name, wanted_primary, wanted_accent)
        RETURNING id
    ),
    reached AS (
        INSERT INTO tenancy.addresses (address, tenant_id)
        SELECT wanted_address, registered.id FROM registered
    )
    SELECT registered.id FROM registered
$$;

-- One tenant for one address, so no caller can list them.
CREATE FUNCTION tenant_at(wanted text)
RETURNS TABLE (
    id bigint, slug text, name text, primary_color text, accent_color text
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT t.id, t.slug, t.name, t.primary_color, t.accent_color
    FROM tenancy.addresses AS a
    INNER JOIN tenancy.tenants AS t ON a.tenant_id = t.id
    WHERE a.address = wanted
$$;

-- Whether some tenant holds one address, for the reserved-name check at
-- start (#90). One address a call, so no caller can test many at once.
CREATE FUNCTION address_in_use(wanted text)
RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT EXISTS (SELECT FROM tenancy.addresses AS a WHERE a.address = wanted)
$$;

REVOKE EXECUTE ON FUNCTION register_tenant(text, text, text, text, text),
tenant_at(text), address_in_use(text) FROM public;
GRANT EXECUTE ON FUNCTION register_tenant(text, text, text, text, text),
tenant_at(text), address_in_use(text) TO tetsu_tenancy;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA tenancy TO tetsu_tenancy_lookup;
ALTER FUNCTION register_tenant(text, text, text, text, text)
OWNER TO tetsu_tenancy_lookup;
ALTER FUNCTION tenant_at(text) OWNER TO tetsu_tenancy_lookup;
ALTER FUNCTION address_in_use(text) OWNER TO tetsu_tenancy_lookup;
REVOKE CREATE ON SCHEMA tenancy FROM tetsu_tenancy_lookup;
