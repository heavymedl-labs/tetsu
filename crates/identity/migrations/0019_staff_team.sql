-- A business's Team page (ADR-0096): its staff and open invites, and the
-- changes its owners and managers make to them, each run as Identity's role
-- in the business's own transaction, under its row security. Identity decides
-- who may make a change from the roles it read; each change here first locks
-- the business's owners, then checks that the roles it was decided on still
-- hold, so changes at one business wait for each other, and two owners
-- demoting each other at once cannot leave it none.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

GRANT SELECT (id, name, email) ON identity.staff TO tetsu_identity_lookup;
GRANT SELECT (tenant_id, staff_id, created_at) ON identity.staff_roles
TO tetsu_identity_lookup;

-- A staff member's own businesses are read before any business is known, to
-- name one on their personal pages (ADR-0096), as the research behind
-- ADR-0089 foresaw.
CREATE POLICY lookup ON identity.staff_roles FOR SELECT TO tetsu_identity_lookup
USING (TRUE);

COMMENT ON POLICY lookup ON identity.staff_roles IS
'tetsu:lookup: a staff member''s own businesses are found before any is known';

-- The business a staff member joined first, which their personal pages name
-- until they can switch between their businesses; none for one who holds no
-- role. Identity's role may call it for any staff member, learning only a
-- business's id, which it may already read every row of.
CREATE FUNCTION identity.staff_first_business(wanted_staff bigint)
RETURNS bigint
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT r.tenant_id
    FROM identity.staff_roles AS r
    WHERE r.staff_id = wanted_staff
    ORDER BY r.created_at, r.tenant_id
    LIMIT 1
$$;
-- Changing a role, and locking roles while a change is checked.
GRANT UPDATE (role_name) ON identity.staff_roles TO tetsu_identity;

-- The name and email of one staff member, for the business they hold a role
-- or an invite at. Identity's role may call it for any staff member; it may
-- already invite any email, so ADR-0061's trust boundary is unchanged, and only
-- staff_team and staff_open_invites call it.
CREATE FUNCTION identity.staff_named(wanted_staff bigint)
RETURNS TABLE (name text, email text)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT o.name, o.email FROM identity.staff AS o WHERE o.id = wanted_staff
$$;

-- This business's staff, the longest here first.
CREATE FUNCTION identity.staff_team()
RETURNS TABLE (
    staff_id bigint,
    name text,
    email text,
    role_name text,
    joined_at timestamptz
)
LANGUAGE sql STABLE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT r.staff_id, n.name, n.email, r.role_name, r.created_at
    FROM identity.staff_roles AS r
    CROSS JOIN LATERAL identity.staff_named(r.staff_id) AS n
    ORDER BY r.created_at, r.staff_id
$$;

-- This business's open invites, the oldest first, by their random ids.
CREATE FUNCTION identity.staff_open_invites()
RETURNS TABLE (
    invite_id text,
    staff_id bigint,
    name text,
    email text,
    role_name text,
    ends_at timestamptz
)
LANGUAGE sql STABLE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT e.id::text, e.staff_id, n.name, n.email, e.role_name, e.ends_at
    FROM identity.staff_invites AS e
    CROSS JOIN LATERAL identity.staff_named(e.staff_id) AS n
    WHERE e.ends_at > statement_timestamp()
    ORDER BY e.issued_at, e.id
$$;

-- The role a staff member holds here, or none, held until the transaction
-- ends so it cannot change under a check.
CREATE FUNCTION identity.staff_role_here(wanted_staff bigint)
RETURNS text
LANGUAGE sql VOLATILE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT r.role_name
    FROM identity.staff_roles AS r
    WHERE r.staff_id = wanted_staff
    FOR UPDATE
$$;

-- Locks this business's owners, in one order, and says whether the staff
-- member acting still holds the role their change was decided on, held until
-- the transaction ends. It refuses any isolation level but read committed
-- with TD002, as require_read_committed does, which only definer functions
-- may call: under another level, a role changed while it waited escapes the
-- check.
CREATE FUNCTION identity.staff_still_acting(
    acting_staff bigint, acting_role text
)
RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
BEGIN
    IF current_setting('transaction_isolation') <> 'read committed' THEN
        RAISE EXCEPTION 'changing a team needs read committed'
        USING ERRCODE = 'TD002';
    END IF;
    PERFORM
    FROM identity.staff_roles AS r
    WHERE r.role_name = 'owner'
    ORDER BY r.staff_id
    FOR UPDATE;
    RETURN identity.staff_role_here(acting_staff) IS NOT DISTINCT FROM acting_role;
END
$$;

-- Whether this business keeps an owner once the staff member it names no
-- longer is one, read while its owners are locked.
CREATE FUNCTION identity.staff_owner_remains(leaving_staff bigint)
RETURNS boolean
LANGUAGE sql STABLE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT EXISTS (
        SELECT FROM identity.staff_roles AS r
        WHERE r.role_name = 'owner' AND r.staff_id <> leaving_staff
    )
$$;

-- Changes a staff member's role here from the one it was decided on, and
-- says what came of it: changed; unchanged, for the role they hold already;
-- stale, when either role no longer holds; or last_owner, which no change
-- leaves without an owner.
CREATE FUNCTION identity.staff_change_role(
    acting_staff bigint,
    acting_role text,
    wanted_staff bigint,
    held_role text,
    wanted_role text
)
RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
BEGIN
    IF
        NOT identity.staff_still_acting(acting_staff, acting_role)
        OR identity.staff_role_here(wanted_staff) IS DISTINCT FROM held_role
    THEN
        RETURN 'stale';
    END IF;
    IF held_role = wanted_role THEN
        RETURN 'unchanged';
    END IF;
    IF
        held_role = 'owner'
        AND NOT identity.staff_owner_remains(wanted_staff)
    THEN
        RETURN 'last_owner';
    END IF;
    UPDATE identity.staff_roles AS r
    SET role_name = wanted_role
    WHERE r.staff_id = wanted_staff;
    RETURN 'changed';
END
$$;

-- Removes a staff member from this business, with any invite of theirs here,
-- from the role it was decided on, and says what came of it: removed, stale,
-- or last_owner, as staff_change_role does.
CREATE FUNCTION identity.staff_remove(
    acting_staff bigint,
    acting_role text,
    wanted_staff bigint,
    held_role text
)
RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
BEGIN
    IF
        NOT identity.staff_still_acting(acting_staff, acting_role)
        OR identity.staff_role_here(wanted_staff) IS DISTINCT FROM held_role
    THEN
        RETURN 'stale';
    END IF;
    IF
        held_role = 'owner'
        AND NOT identity.staff_owner_remains(wanted_staff)
    THEN
        RETURN 'last_owner';
    END IF;
    DELETE FROM identity.staff_roles AS r WHERE r.staff_id = wanted_staff;
    DELETE FROM identity.staff_invites AS e WHERE e.staff_id = wanted_staff;
    RETURN 'removed';
END
$$;

-- Cancels an open invite here, by its random id, from the role it was decided
-- on, and says what came of it: cancelled, or stale, for one that ended, gives
-- another role now, or a role the staff member acting no longer holds.
CREATE FUNCTION identity.staff_cancel_invite(
    acting_staff bigint,
    acting_role text,
    wanted_invite text,
    held_role text
)
RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
BEGIN
    IF NOT identity.staff_still_acting(acting_staff, acting_role) THEN
        RETURN 'stale';
    END IF;
    DELETE FROM identity.staff_invites AS e
    WHERE
        e.id = wanted_invite::uuid
        AND e.role_name = held_role
        AND e.ends_at > statement_timestamp();
    IF FOUND THEN
        RETURN 'cancelled';
    END IF;
    RETURN 'stale';
END
$$;

-- Invites someone here from the Team page, as staff_issue_invite does, once
-- the role it was decided on still holds, and only over an open invite of
-- theirs here that gives one of the roles the staff member acting reaches, so
-- no one replaces an invite above them; stale, or beyond, with nothing else,
-- when either does not hold.
CREATE FUNCTION identity.staff_invite_from(
    acting_staff bigint,
    acting_role text,
    reachable text[],
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
    replaced text;
BEGIN
    IF NOT identity.staff_still_acting(acting_staff, acting_role) THEN
        outcome := 'stale';
        RETURN;
    END IF;
    SELECT e.role_name INTO replaced
    FROM identity.staff_invites AS e
    CROSS JOIN LATERAL identity.staff_named(e.staff_id) AS n
    WHERE
        lower(n.email) = lower(wanted_email)
        AND e.ends_at > statement_timestamp()
    FOR UPDATE OF e;
    IF replaced IS NOT NULL AND replaced <> ALL (reachable) THEN
        outcome := 'beyond';
        RETURN;
    END IF;
    SELECT i.outcome, i.invited, i.created, i.renamed
    INTO outcome, invited, created, renamed
    FROM identity.staff_issue_invite(
        wanted_link, wanted_name, wanted_email, wanted_role
    ) AS i;
END
$$;

GRANT EXECUTE ON FUNCTION identity.staff_named(bigint),
identity.staff_first_business(bigint),
identity.staff_team(),
identity.staff_open_invites(),
identity.staff_role_here(bigint),
identity.staff_still_acting(bigint, text),
identity.staff_owner_remains(bigint),
identity.staff_change_role(bigint, text, bigint, text, text),
identity.staff_remove(bigint, text, bigint, text),
identity.staff_cancel_invite(bigint, text, text, text),
identity.staff_invite_from(bigint, text, text[], bytea, text, text, text)
TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION identity.staff_named(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.staff_first_business(bigint)
OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;

-- A limit on the invites each staff member sends from the Team page, since
-- each sends an email: twenty an hour, keyed on the staff member rather than
-- their network, so a shared front desk's network spends no one else's room.
-- The new kind is checked for new rows at once and for old ones by 0020, so
-- the table is never locked against writes while it is scanned. Widening the
-- check repeats 0010's exception to the migration lint, for the same reason:
-- only Identity's own functions read and write these counters.
-- squawk-ignore ban-drop-constraint
ALTER TABLE limits DROP CONSTRAINT limits_kind_check;
ALTER TABLE limits ADD CONSTRAINT limits_kind_check CHECK (
    kind IN (
        'login_begin', 'login_failure', 'join', 'refusal', 'refusal_flood',
        'staff_login_begin', 'staff_login_failure', 'staff_join',
        'staff_invite'
    )
) NOT VALID;

CREATE OR REPLACE FUNCTION limit_rule(
    wanted_kind text,
    OUT span interval,
    OUT room integer,
    OUT first_wait interval,
    OUT longest interval
)
LANGUAGE sql IMMUTABLE
SET search_path = pg_catalog, pg_temp
AS $$
    WITH rules (kind, span, room, first_wait, longest) AS (
        VALUES
        ('refusal', interval '15 minutes', 10, NULL::interval, NULL::interval),
        ('refusal_flood', interval '15 minutes', 1, NULL, NULL),
        ('join', interval '1 hour', 10, NULL, NULL),
        ('login_failure', interval '15 minutes', 10, interval '1 minute',
            interval '1 hour'),
        ('login_begin', interval '15 minutes', 30, NULL, NULL),
        ('staff_invite', interval '1 hour', 20, NULL, NULL)
    )

    SELECT rules.span, rules.room, rules.first_wait, rules.longest
    FROM rules
    WHERE
        rules.kind = wanted_kind
        OR 'staff_' || rules.kind = wanted_kind
        AND rules.kind IN ('login_begin', 'login_failure', 'join')
$$;
