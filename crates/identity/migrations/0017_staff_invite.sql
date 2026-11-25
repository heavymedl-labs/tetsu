-- Issuing a staff invite (ADR-0089), for `tetsu staff invite`: find the
-- staff member an email names, or make one with no passkey, then, in the
-- business's own transaction, replace any open invite of theirs there with a
-- new one, good for seven days. Finding or making the staff member is global
-- and runs as the lookup role; the invite belongs to the business, so it is
-- written as Identity's role under the business's row security.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

GRANT SELECT (email), INSERT (name, email) ON identity.staff
TO tetsu_identity_lookup;

-- The staff member an email names, whatever its case, made with no passkey
-- if there is none: who they are, whether they were made now, whether they
-- are disabled, and whether their stored name differs from the one given,
-- which only they change (ADR-0040). It holds their row in share mode until
-- the transaction ends, so a disable waits for the invite. Identity's role
-- may call it directly, learning those answers for any email and making a
-- staff member unrecorded; it may already issue any invite, so ADR-0061's
-- trust boundary is unchanged, and only staff_issue_invite calls it.
CREATE FUNCTION identity.staff_enlist(
    wanted_name text,
    wanted_email text,
    OUT who bigint,
    OUT created boolean,
    OUT disabled boolean,
    OUT renamed boolean
)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
BEGIN
    PERFORM identity.require_read_committed('enlisting a staff member');
    INSERT INTO identity.staff (name, email)
    VALUES (wanted_name, wanted_email)
    ON CONFLICT ((lower(email))) DO NOTHING
    RETURNING id INTO who;
    created := FOUND;
    IF created THEN
        disabled := FALSE;
        renamed := FALSE;
        RETURN;
    END IF;
    -- Under read committed, this statement sees the row whose insert the
    -- conflict waited for.
    SELECT o.id, o.disabled_at IS NOT NULL, o.name <> wanted_name
    INTO who, disabled, renamed
    FROM identity.staff AS o
    WHERE lower(o.email) = lower(wanted_email)
    FOR SHARE;
END
$$;

-- Invites the staff member an email names to this business with a role,
-- under a link's hash, good once and for seven days, and says what came of
-- it: issued, replacing any open invite of theirs here, whose link then
-- joins no one, or one issued at the same moment; disabled, for a staff
-- member who is; or member, for one who holds a role here already, whose role
-- only sub-slice 4 changes. Either refusal writes nothing here. Run as
-- Identity's role in the business's own transaction, so row security keeps
-- every write to this business.
CREATE FUNCTION identity.staff_issue_invite(
    wanted_link bytea,
    wanted_name text,
    wanted_email text,
    wanted_role text,
    OUT outcome text,
    OUT invited bigint,
    OUT created boolean,
    OUT renamed boolean
)
LANGUAGE plpgsql VOLATILE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    disabled boolean;
    at timestamptz;
BEGIN
    SELECT e.who, e.created, e.disabled, e.renamed
    INTO invited, created, disabled, renamed
    FROM identity.staff_enlist(wanted_name, wanted_email) AS e;
    IF disabled THEN
        outcome := 'disabled';
        RETURN;
    END IF;
    -- A join or an acceptance at this business spends the person's open
    -- invite, so waiting on that invite's row waits for it, and the role
    -- check after, a statement of its own, sees any role it gave.
    PERFORM FROM identity.staff_invites AS e
    WHERE e.staff_id = invited
    FOR UPDATE;
    IF EXISTS (
        SELECT FROM identity.staff_roles AS r WHERE r.staff_id = invited
    ) THEN
        outcome := 'member';
        RETURN;
    END IF;
    at := clock_timestamp();
    -- Replacing the open invite in place, so an invite at the same moment
    -- that found none waits on this one's row and replaces it in turn.
    INSERT INTO identity.staff_invites
        (tenant_id, token_hash, staff_id, role_name, issued_at, ends_at)
    VALUES (
        (SELECT nullif(current_setting('app.tenant_id', TRUE), '')::bigint),
        wanted_link, invited, wanted_role, at, at + interval '7 days'
    )
    ON CONFLICT (tenant_id, staff_id) DO UPDATE
        SET
            token_hash = excluded.token_hash,
            role_name = excluded.role_name,
            issued_at = excluded.issued_at,
            ends_at = excluded.ends_at;
    outcome := 'issued';
END
$$;

-- Replacing an invite in place updates its row.
GRANT UPDATE (token_hash, role_name, issued_at, ends_at)
ON identity.staff_invites TO tetsu_identity;

GRANT EXECUTE ON FUNCTION identity.staff_enlist(text, text),
identity.staff_issue_invite(bytea, text, text, text)
TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION identity.staff_enlist(text, text) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
