-- Rate limits by network address (ADR-0072): login begins, failed logins, and
-- joining, each counted per requester's network. A passkey cannot be guessed,
-- so they guard the server and the ceremony table, and slow a flood rather
-- than lock anyone out. They replace the bounds begin_login keeps as their
-- backstop.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- The secret each requester's network is hashed with, made here once, so the
-- limits hold no address (ADR-0045). Two random UUIDs give 244 random bits.
CREATE TABLE limit_secret (
    only_row boolean PRIMARY KEY DEFAULT TRUE CHECK (only_row),
    secret bytea NOT NULL CHECK (octet_length(secret) = 32)
);

COMMENT ON TABLE limit_secret IS
'tetsu:global: one secret for every network, before any business is known';

INSERT INTO limit_secret (secret)
VALUES (
    sha256(
        convert_to(gen_random_uuid()::text || gen_random_uuid()::text, 'UTF8')
    )
);

-- One row a limit and network. A limit has room for a number of attempts in a
-- span, and each attempt takes an equal share of it: due is when all its room
-- is free again, so an attempt fits while due, plus its share, is at most a
-- span away (GCRA). Failures that meet their limit are struck, each strike
-- doubling the wait, until due passes and the row is forgotten.
CREATE TABLE limits (
    kind text NOT NULL
    CHECK (kind IN ('login_begin', 'login_failure', 'join')),
    network_hash bytea NOT NULL CHECK (octet_length(network_hash) = 32),
    due timestamptz NOT NULL,
    strikes bigint NOT NULL DEFAULT 0 CHECK (strikes >= 0),
    PRIMARY KEY (kind, network_hash)
);

COMMENT ON TABLE limits IS
'tetsu:global: a network is counted before any business is known';

CREATE INDEX limits_due ON limits (due);

-- The runner's default privileges give Identity's role every right on a new
-- table; it reaches these only through the functions below.
REVOKE ALL ON limit_secret, limits FROM tetsu_identity;
GRANT SELECT ON limit_secret TO tetsu_identity_lookup;
GRANT SELECT, INSERT, DELETE ON limits TO tetsu_identity_lookup;
GRANT UPDATE (due, strikes) ON limits TO tetsu_identity_lookup;

-- Each limit's numbers, beside the clock that applies them, so no caller can
-- pass others (ADR-0072): its span, the attempts that fit in it, and for
-- failures, the first wait past it and the longest. None for another kind.
CREATE FUNCTION limit_rule(
    wanted_kind text,
    OUT span interval,
    OUT room integer,
    OUT first_wait interval,
    OUT longest interval
)
LANGUAGE sql IMMUTABLE
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT r.span, r.room, r.first_wait, r.longest
    FROM (
        VALUES
        ('login_begin', interval '15 minutes', 30, NULL::interval,
            NULL::interval),
        ('login_failure', interval '15 minutes', 10, interval '1 minute',
            interval '1 hour'),
        ('join', interval '1 hour', 10, NULL, NULL)
    ) AS r (kind, span, room, first_wait, longest)
    WHERE r.kind = wanted_kind
$$;

-- The row a limit keeps for a network, with the secret's hash of it, after
-- sweeping up to a hundred rows a day past their use. A new row is free.
CREATE FUNCTION limit_row(
    wanted_kind text, network bytea, at timestamptz
)
RETURNS limits
LANGUAGE plpgsql VOLATILE
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    found identity.limits;
BEGIN
    IF octet_length(network) IS DISTINCT FROM 16 THEN
        RAISE EXCEPTION 'a network is 16 bytes';
    END IF;
    DELETE FROM identity.limits AS l
    WHERE l.ctid = any(array(
        SELECT stale.ctid
        FROM identity.limits AS stale
        WHERE stale.due < at - interval '1 day'
        LIMIT 100
        FOR UPDATE SKIP LOCKED
    ));
    INSERT INTO identity.limits AS l (kind, network_hash, due)
    SELECT wanted_kind, sha256(s.secret || network), at
    FROM identity.limit_secret AS s
    ON CONFLICT (kind, network_hash) DO NOTHING;
    SELECT l.* INTO STRICT found
    FROM identity.limits AS l
    CROSS JOIN identity.limit_secret AS s
    WHERE l.kind = wanted_kind AND l.network_hash = sha256(s.secret || network)
    FOR UPDATE OF l;
    RETURN found;
END
$$;

-- Admits an attempt from a network under a limit, or answers the whole
-- seconds to wait. A limit of attempts counts this one; the limit of failures
-- only checks, and fail counts them. Read committed only (TD002), so a sweep
-- never waits on a row another transaction holds.
CREATE FUNCTION admit(wanted_kind text, network bytea)
RETURNS bigint
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz := clock_timestamp();
    rule record;
    share interval;
    kept identity.limits;
    free_from timestamptz;
BEGIN
    PERFORM identity.require_read_committed('admitting an attempt');
    SELECT r.* INTO rule FROM identity.limit_rule(wanted_kind) AS r;
    IF rule.room IS NULL THEN
        RAISE EXCEPTION 'no limit is called %', wanted_kind;
    END IF;
    share := rule.span / rule.room;
    kept := identity.limit_row(wanted_kind, network, at);
    free_from := greatest(kept.due, at);
    IF free_from + share - at > rule.span THEN
        RETURN ceil(extract(EPOCH FROM free_from + share - rule.span - at));
    END IF;
    IF rule.first_wait IS NULL THEN
        UPDATE identity.limits AS l SET due = free_from + share
        WHERE l.kind = kept.kind AND l.network_hash = kept.network_hash;
    END IF;
    RETURN NULL;
END
$$;

-- Counts a failure from a network under the limit of failures. One that
-- leaves no room for the next is struck: the next waits the first wait,
-- doubled for each strike before it, up to the longest. Strikes are forgotten
-- once due passes.
CREATE FUNCTION fail(wanted_kind text, network bytea)
RETURNS void
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz := clock_timestamp();
    rule record;
    share interval;
    kept identity.limits;
    struck bigint;
    next_due timestamptz;
BEGIN
    PERFORM identity.require_read_committed('counting a failure');
    SELECT r.* INTO rule FROM identity.limit_rule(wanted_kind) AS r;
    IF rule.first_wait IS NULL THEN
        RAISE EXCEPTION 'no limit of failures is called %', wanted_kind;
    END IF;
    share := rule.span / rule.room;
    kept := identity.limit_row(wanted_kind, network, at);
    struck := CASE WHEN kept.due <= at THEN 0 ELSE kept.strikes END;
    next_due := greatest(kept.due, at) + share;
    IF next_due + share - at > rule.span THEN
        struck := struck + 1;
        next_due := at + rule.span - share
            + least(rule.first_wait * 2 ^ (struck - 1), rule.longest);
    END IF;
    UPDATE identity.limits AS l SET due = next_due, strikes = struck
    WHERE l.kind = kept.kind AND l.network_hash = kept.network_hash;
END
$$;

REVOKE EXECUTE ON FUNCTION limit_rule(text),
limit_row(text, bytea, timestamptz), admit(text, bytea), fail(text, bytea)
FROM public;
GRANT EXECUTE ON FUNCTION admit(text, bytea), fail(text, bytea)
TO tetsu_identity;
-- The two helpers run as whoever calls them, which is the functions' owner.
GRANT EXECUTE ON FUNCTION limit_rule(text), limit_row(text, bytea, timestamptz)
TO tetsu_identity_lookup;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION admit(text, bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION fail(text, bytea) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
