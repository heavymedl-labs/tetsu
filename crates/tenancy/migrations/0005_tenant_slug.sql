-- A tenant's slug by its id, for a page that links to a business it knows only
-- by id, such as a staff member's link to their business's Team page
-- (ADR-0096). Counting up the ids lists every slug, which each business's own
-- address already shows (ADR-0042).
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE FUNCTION tenant_slug(wanted bigint)
RETURNS text
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT t.slug FROM tenancy.tenants AS t WHERE t.id = wanted
$$;

REVOKE EXECUTE ON FUNCTION tenant_slug(bigint) FROM public;
GRANT EXECUTE ON FUNCTION tenant_slug(bigint) TO tetsu_tenancy;

GRANT CREATE ON SCHEMA tenancy TO tetsu_tenancy_lookup;
ALTER FUNCTION tenant_slug(bigint) OWNER TO tetsu_tenancy_lookup;
REVOKE CREATE ON SCHEMA tenancy FROM tetsu_tenancy_lookup;
