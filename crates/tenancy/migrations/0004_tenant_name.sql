-- A tenant's name by its id, for a page that names a business it knows only
-- by id, such as a staff invite's (#111). Counting up the ids lists every
-- name, which each business's own page already shows (ADR-0042).
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE FUNCTION tenant_name(wanted bigint)
RETURNS text
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT t.name FROM tenancy.tenants AS t WHERE t.id = wanted
$$;

REVOKE EXECUTE ON FUNCTION tenant_name(bigint) FROM public;
GRANT EXECUTE ON FUNCTION tenant_name(bigint) TO tetsu_tenancy;

GRANT CREATE ON SCHEMA tenancy TO tetsu_tenancy_lookup;
ALTER FUNCTION tenant_name(bigint) OWNER TO tetsu_tenancy_lookup;
REVOKE CREATE ON SCHEMA tenancy FROM tetsu_tenancy_lookup;
