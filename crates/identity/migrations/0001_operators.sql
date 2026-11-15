-- Operators, who work for Tetsu itself, and sessions (ADR-0040, ADR-0048).
-- Both are global: an operator belongs to no business, and a session is
-- found by its token before any business is known.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE TABLE operators (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name text NOT NULL,
    -- WebAuthn's user handle: random, so it says nothing about the operator
    -- (ADR-0040).
    user_handle uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    created_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE operators IS
'tetsu:global: operators work for Tetsu, not a business';

-- A session is kept under its token's SHA-256 hash, never the token
-- (ADR-0047).
CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    realm text NOT NULL CHECK (realm IN ('operator')),
    operator_id bigint NOT NULL REFERENCES operators (id),
    started_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL
);

COMMENT ON TABLE sessions IS
'tetsu:global: found by its token before any business is known';

CREATE INDEX sessions_operator_id ON sessions (operator_id);

-- The runner's default privileges give Identity's role every right on a new
-- table; each is replaced by exactly what is granted below.
REVOKE ALL ON operators, sessions FROM tetsu_identity;
REVOKE ALL ON SEQUENCE operators_id_seq FROM tetsu_identity;

-- Identity's role adds and reads operators, and reaches sessions only through
-- the functions below, one token's hash a call, so no caller can list them.
GRANT SELECT, INSERT ON operators TO tetsu_identity;
GRANT SELECT, INSERT, UPDATE, DELETE ON sessions TO tetsu_identity_lookup;

-- Each realm's limits live here, beside the clock that applies them, so no
-- caller can lengthen a session by passing other numbers (ADR-0048). Every
-- function reads the clock once, as each statement runs, so neither a
-- server's clock nor a request handled late can stretch a session; a
-- statement that waits for another's lock on the same row is judged as of its
-- own start, a wait no longer than the lock timeout.

-- Starts a session now, and removes the operator's spent sessions in the
-- realm, so sessions never presented again do not pile up.
CREATE FUNCTION start_session(
    wanted_hash bytea, wanted_realm text, wanted_operator bigint
)
RETURNS void
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH clock AS (
        SELECT
            clock_timestamp() AS at,
            CASE wanted_realm WHEN 'operator' THEN interval '8 hours' END
                AS absolute,
            CASE wanted_realm WHEN 'operator' THEN interval '15 minutes' END
                AS idle
    ),

    spent AS (
        DELETE FROM identity.sessions AS s
        USING clock
        WHERE
            s.operator_id = wanted_operator
            AND s.realm = wanted_realm
            AND (clock.at >= s.ends_at OR clock.at >= s.last_seen_at + clock.idle)
    )

    INSERT INTO identity.sessions
        (token_hash, realm, operator_id, started_at, last_seen_at, ends_at)
    SELECT
        wanted_hash,
        wanted_realm,
        wanted_operator,
        clock.at,
        clock.at,
        clock.at + clock.absolute
    FROM clock
$$;

-- Records a request on the live session a token's hash names in a realm, and
-- returns it; a session past either limit is removed instead, and none is
-- returned. The clock is read once. The update waits for any request holding
-- the row and checks it again as that request left it, so requests at the
-- same moment never end a live session; only a session the update did not
-- find live is removed, and only if it is spent, so none is revived, and the
-- last request never moves back. Only the realm asked for counts, so a token
-- from another realm never passes as this one's (ADR-0040, ADR-0048).
CREATE FUNCTION use_session(wanted_hash bytea, wanted_realm text)
RETURNS TABLE (
    realm text,
    operator_id bigint,
    started_at timestamptz,
    last_seen_at timestamptz,
    ends_at timestamptz
)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz := clock_timestamp();
    idle interval := CASE wanted_realm
        WHEN 'operator' THEN interval '15 minutes'
    END;
BEGIN
    RETURN QUERY
    UPDATE identity.sessions AS s
    SET last_seen_at = greatest(s.last_seen_at, at)
    WHERE
        s.token_hash = wanted_hash
        AND s.realm = wanted_realm
        AND at < s.ends_at
        AND at < s.last_seen_at + idle
    RETURNING s.realm, s.operator_id, s.started_at, s.last_seen_at, s.ends_at;
    IF NOT FOUND THEN
        DELETE FROM identity.sessions AS s
        WHERE
            s.token_hash = wanted_hash
            AND s.realm = wanted_realm
            AND (at >= s.ends_at OR at >= s.last_seen_at + idle);
    END IF;
END
$$;

-- Removes a session in a realm, and says whether it was live when it ended: a
-- spent one is removed too, but it had already ended.
CREATE FUNCTION end_session(wanted_hash bytea, wanted_realm text)
RETURNS boolean
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH clock AS (
        SELECT
            clock_timestamp() AS at,
            CASE wanted_realm WHEN 'operator' THEN interval '15 minutes' END
                AS idle
    ),

    ended AS (
        DELETE FROM identity.sessions AS s
        USING clock
        WHERE s.token_hash = wanted_hash AND s.realm = wanted_realm
        RETURNING clock.at < s.ends_at AND clock.at < s.last_seen_at + clock.idle
            AS live
    )

    SELECT coalesce(bool_or(ended.live), false) FROM ended
$$;

REVOKE EXECUTE ON FUNCTION start_session(bytea, text, bigint),
use_session(bytea, text), end_session(bytea, text) FROM public;
GRANT EXECUTE ON FUNCTION start_session(bytea, text, bigint),
use_session(bytea, text), end_session(bytea, text) TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION start_session(bytea, text, bigint)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION use_session(bytea, text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_session(bytea, text) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
