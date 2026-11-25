-- What only staff do to join a business (ADR-0089): find an invite by its
-- link before its business is known, join by it or accept it, and in the same
-- transaction use it up and add the role it gives. Joining by a link, for
-- someone with no passkey yet, is the login flow's, rendered for staff in
-- version 15. The invite and the role belong to the business, so staff_join
-- and staff_accept_invite run as Identity's role in that business's own
-- transaction, under its row security, and the lookups they call only read
-- invites, through the policy 0013 gives them (ADR-0042, as clarified on
-- 2026-11-25): the check and the writes commit together or not at all.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

GRANT SELECT (tenant_id, role_name) ON identity.staff_invites
TO tetsu_identity_lookup;

-- The invite an open link names, with its business, the staff member it
-- names, the user handle and name a registration needs, the role it gives,
-- and whether they have joined already, so someone with a passkey is asked to
-- log in and accept rather than register; nothing for a link that ended, was
-- replaced, or names someone disabled. Reading a link uses nothing up.
CREATE FUNCTION identity.staff_invitee(wanted_link bytea)
RETURNS TABLE (
    tenant_id bigint,
    staff_id bigint,
    user_handle bytea,
    name text,
    role_name text,
    joined boolean
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT
        e.tenant_id,
        o.id,
        uuid_send(o.user_handle),
        o.name,
        e.role_name,
        EXISTS (
            SELECT FROM identity.staff_passkeys AS p WHERE p.staff_id = o.id
        )
    FROM identity.staff_invites AS e
    INNER JOIN identity.staff AS o ON e.staff_id = o.id
    WHERE
        e.token_hash = wanted_link
        AND e.ends_at > statement_timestamp()
        AND o.disabled_at IS NULL
$$;

-- Whether the staff member whose live session a token's hash names may accept
-- the invite an open link names, and says why not: accepted, with the
-- invite's business and role, for one a passkey proved within ten minutes
-- (ADR-0048, ADR-0069) whom the invite names; logged_out, for no live staff
-- session; disabled; stale, for a login over ten minutes old, or never proven;
-- expired, for a link that ended or was replaced; or other, for an invite that
-- names someone else, which leaves it for them. It uses nothing up;
-- staff_accept_invite does, in the same transaction. It holds the staff
-- member's row in share mode until that transaction ends, so a disable waits
-- for it, and reads the clock after that wait, under read committed only
-- (TD002).
CREATE FUNCTION identity.staff_accept(
    wanted_link bytea,
    wanted_session bytea,
    OUT outcome text,
    OUT tenant_id bigint,
    OUT role_name text
)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    who bigint;
    named bigint;
    fresh boolean;
BEGIN
    PERFORM identity.require_read_committed('accepting an invite');
    SELECT s.staff_id INTO who
    FROM identity.staff_sessions AS s
    WHERE s.token_hash = wanted_session AND s.realm = 'staff';
    PERFORM
    FROM identity.staff AS o
    WHERE o.id = who AND o.disabled_at IS NULL
    FOR SHARE;
    IF who IS NOT NULL AND NOT FOUND THEN
        outcome := 'disabled';
        RETURN;
    END IF;
    at := clock_timestamp();
    SELECT s.proven_at > at - interval '10 minutes' INTO fresh
    FROM identity.staff_sessions AS s
    WHERE
        s.token_hash = wanted_session
        AND s.staff_id = who
        AND at < s.ends_at
        AND at < s.last_seen_at + identity.idle_limit(s.realm);
    IF NOT FOUND THEN
        outcome := 'logged_out';
        RETURN;
    ELSIF fresh IS NOT TRUE THEN
        outcome := 'stale';
        RETURN;
    END IF;
    SELECT e.staff_id, e.tenant_id, e.role_name
    INTO named, tenant_id, role_name
    FROM identity.staff_invites AS e
    WHERE e.token_hash = wanted_link AND e.ends_at > at;
    outcome := CASE
        WHEN named IS NULL THEN 'expired'
        WHEN named = who THEN 'accepted'
        ELSE 'other'
    END;
    IF outcome <> 'accepted' THEN
        tenant_id := null;
        role_name := null;
    END IF;
END
$$;

-- Joins someone with no passkey yet by their invite link, as
-- staff_join_by_link does, and then, in this business's transaction, uses the
-- invite up and gives them its role, saying what came of it as
-- staff_join_by_link does. An invite this business does not hold, or one that
-- ended or was replaced meanwhile, undoes the join, and says expired. Run as
-- Identity's role, so row security keeps every write to this business.
CREATE FUNCTION identity.staff_join(
    wanted_link bytea,
    wanted_staff bigint,
    wanted_credential bytea,
    wanted_passkey jsonb,
    wanted_transports text,
    wanted_session bytea,
    wanted_presented bytea
)
RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    outcome text;
BEGIN
    BEGIN
        outcome := identity.staff_join_by_link(
            wanted_link, wanted_staff, wanted_credential, wanted_passkey,
            wanted_transports, wanted_session, wanted_presented
        );
        IF outcome = 'redeemed' THEN
            PERFORM identity.staff_spend_invite(wanted_link, wanted_staff);
        END IF;
    EXCEPTION WHEN SQLSTATE 'TD004' THEN
        RETURN 'expired';
    END;
    RETURN outcome;
END
$$;

-- Accepts an invite for the staff member whose session a token's hash names,
-- as staff_accept checks, and then, in this business's transaction, uses it
-- up and gives them its role, saying what came of it as staff_accept does,
-- or member, for someone who holds a role here already, whose invite is used
-- up all the same. An invite this business does not hold, or one that ended
-- or was replaced meanwhile, says expired, and nothing is written. Run as
-- Identity's role, so row security keeps every write to this business.
CREATE FUNCTION identity.staff_accept_invite(
    wanted_link bytea,
    wanted_session bytea
)
RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    outcome text;
BEGIN
    SELECT a.outcome INTO outcome
    FROM identity.staff_accept(wanted_link, wanted_session) AS a;
    IF outcome <> 'accepted' THEN
        RETURN outcome;
    END IF;
    BEGIN
        IF NOT identity.staff_spend_invite(wanted_link, null) THEN
            outcome := 'member';
        END IF;
    EXCEPTION WHEN SQLSTATE 'TD004' THEN
        RETURN 'expired';
    END;
    RETURN outcome;
END
$$;

-- Uses up the open invite a link names at this business, for the staff
-- member it names, if one is given, and gives them its role; says whether the
-- role is new, false for someone who holds one here already. No such invite
-- raises TD004, so its caller can undo what it did first. Run as Identity's
-- role, under this business's row security.
CREATE FUNCTION identity.staff_spend_invite(
    wanted_link bytea, wanted_staff bigint
)
RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY INVOKER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    business bigint;
    who bigint;
    given text;
BEGIN
    DELETE FROM identity.staff_invites AS e
    WHERE
        e.token_hash = wanted_link
        AND (wanted_staff IS NULL OR e.staff_id = wanted_staff)
        AND e.ends_at > clock_timestamp()
    RETURNING e.tenant_id, e.staff_id, e.role_name INTO business, who, given;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'no open invite at this business' USING ERRCODE = 'TD004';
    END IF;
    INSERT INTO identity.staff_roles (tenant_id, staff_id, role_name)
    VALUES (business, who, given)
    ON CONFLICT (tenant_id, staff_id) DO NOTHING;
    RETURN FOUND;
END
$$;

-- Identity's role runs the lookup and the two ways to join; the definer
-- functions they call and the spending they share run as whoever calls them,
-- so it may run those too.
GRANT EXECUTE ON FUNCTION identity.staff_invitee(bytea),
identity.staff_join(bytea, bigint, bytea, jsonb, text, bytea, bytea),
identity.staff_accept_invite(bytea, bytea),
identity.staff_accept(bytea, bytea),
identity.staff_join_by_link(bytea, bigint, bytea, jsonb, text, bytea, bytea),
identity.staff_spend_invite(bytea, bigint)
TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION identity.staff_invitee(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.staff_accept(bytea, bytea)
OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
