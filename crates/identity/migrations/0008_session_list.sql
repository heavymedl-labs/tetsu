-- An admin's own sessions: listed by device and time, and ended from their
-- page (ADR-0059, v5.0.0-7.5.2, #145).
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- The device a session started on, as coarsely as the list of sessions shows
-- it, such as Firefox on Linux, and none when the browser named none. It goes
-- with the session (ADR-0059).
ALTER TABLE admin_sessions ADD COLUMN device text
CHECK (device IS NULL OR length(device) BETWEEN 1 AND 64);
GRANT SELECT (id, device), UPDATE (device) ON admin_sessions
TO tetsu_identity_lookup;

-- Names the device a session just started on, once: login and joining call it
-- with the session they start, in the same statement. A session already named
-- is left as it is.
CREATE FUNCTION describe_session(wanted_hash bytea, wanted_device text)
RETURNS void
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    UPDATE identity.admin_sessions AS s
    SET device = wanted_device
    WHERE s.token_hash = wanted_hash AND s.device IS NULL
$$;

-- The live admin sessions of whoever's live session a token's hash names,
-- the presented one first and then the most recently used: each by its random
-- id, never its token's hash, with when it started and was last used, and its
-- device. The admin is found from the presented session alone, so no request
-- lists another admin's sessions. It renews nothing. Its idle limit must
-- match use_session's, as account's does; #142 gives the limits one home.
CREATE FUNCTION sessions_of(wanted_session bytea)
RETURNS TABLE (
    id text,
    started_at timestamptz,
    last_seen_at timestamptz,
    device text,
    presented boolean
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT
        s.id::text,
        s.started_at,
        s.last_seen_at,
        s.device,
        s.token_hash = wanted_session
    FROM identity.admin_sessions AS mine
    INNER JOIN identity.admins AS o ON mine.admin_id = o.id
    INNER JOIN identity.admin_sessions AS s
        ON mine.admin_id = s.admin_id AND mine.realm = s.realm
    WHERE
        mine.token_hash = wanted_session
        AND mine.realm = 'admin'
        AND statement_timestamp() < mine.ends_at
        AND statement_timestamp() < mine.last_seen_at + interval '15 minutes'
        AND o.disabled_at IS NULL
        AND statement_timestamp() < s.ends_at
        AND statement_timestamp() < s.last_seen_at + interval '15 minutes'
    ORDER BY s.token_hash = wanted_session DESC, s.last_seen_at DESC, s.id
$$;

-- Ends sessions of the admin whose live session a token's hash names: the
-- one with the random id given, or every other one when none is given, never
-- the presented one, which logging out ends. Only a session a passkey proved
-- within ten minutes may (ADR-0048, ADR-0059, v5.0.0-7.5.1). Answers, for the
-- audit record, the admin, the presented session's public id, and either
-- how many ended were live, spent ones going too, or why none could: ended for
-- a session that is not live, disabled for an admin disabled, and stale for
-- a login over ten minutes old. It takes the admin's row first, as
-- end_sessions does, so a login holding it finishes before, under read
-- committed only (TD002).
CREATE FUNCTION end_other_sessions(
    wanted_session bytea,
    wanted_id uuid,
    OUT who bigint,
    OUT asking text,
    OUT ended bigint,
    OUT reason text
)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    fresh boolean;
BEGIN
    PERFORM identity.require_read_committed('ending other sessions');
    SELECT s.admin_id INTO who
    FROM identity.admin_sessions AS s
    WHERE s.token_hash = wanted_session AND s.realm = 'admin';
    IF who IS NULL THEN
        reason := 'ended';
        RETURN;
    END IF;
    PERFORM
    FROM identity.admins AS o
    WHERE o.id = who AND o.disabled_at IS NULL
    FOR NO KEY UPDATE;
    IF NOT FOUND THEN
        reason := 'disabled';
        RETURN;
    END IF;
    at := clock_timestamp();
    SELECT s.id::text, s.proven_at > at - interval '10 minutes'
    INTO asking, fresh
    FROM identity.admin_sessions AS s
    WHERE
        s.token_hash = wanted_session
        AND s.realm = 'admin'
        AND at < s.ends_at
        AND at < s.last_seen_at + interval '15 minutes';
    IF asking IS NULL THEN
        reason := 'ended';
        RETURN;
    END IF;
    IF fresh IS NOT TRUE THEN
        reason := 'stale';
        RETURN;
    END IF;
    WITH gone AS (
        DELETE FROM identity.admin_sessions AS s
        WHERE
            s.admin_id = who
            AND s.realm = 'admin'
            AND s.token_hash <> wanted_session
            AND (wanted_id IS NULL OR s.id = wanted_id)
        RETURNING
            at < s.ends_at AND at < s.last_seen_at + interval '15 minutes'
                AS live
    )

    SELECT count(*) FILTER (WHERE gone.live) INTO ended FROM gone;
END
$$;

REVOKE EXECUTE ON FUNCTION describe_session(bytea, text),
sessions_of(bytea), end_other_sessions(bytea, uuid) FROM public;
GRANT EXECUTE ON FUNCTION describe_session(bytea, text),
sessions_of(bytea), end_other_sessions(bytea, uuid) TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION describe_session(bytea, text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION sessions_of(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_other_sessions(bytea, uuid) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
