-- A staff member's address, for the notices of their logins and new passkeys
-- (ADR-0074), which Identity sends from the dispatcher, with no session to
-- read it through. Identity's role may ask for any staff member's address by
-- their ID, so may list them all, as it may admins' (0005); notices cannot
-- be sent otherwise, and an event never carries an address (ADR-0042,
-- ADR-0045).
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE FUNCTION identity.staff_email(wanted_staff bigint)
RETURNS text
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT o.email FROM identity.staff AS o WHERE o.id = wanted_staff
$$;

GRANT EXECUTE ON FUNCTION identity.staff_email(bigint) TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION identity.staff_email(bigint) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
