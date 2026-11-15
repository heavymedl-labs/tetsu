-- What the operator command does to accounts and sessions (ADR-0040,
-- ADR-0059): enrollment links, disabling an operator, and ending sessions.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- An operator is disabled rather than removed, so the audit record keeps a
-- name to point at. A disabled operator starts no session and is issued no
-- enrollment link.
ALTER TABLE operators ADD COLUMN disabled_at timestamptz;

-- Each operator's one open enrollment link, kept under its token's SHA-256
-- hash, never the token, and good for an hour. A new link replaces the last.
CREATE TABLE enrollments (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    operator_id bigint NOT NULL UNIQUE REFERENCES operators (id),
    issued_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL
);

COMMENT ON TABLE enrollments IS
'tetsu:global: found by its token before anyone is signed in';

REVOKE ALL ON enrollments FROM tetsu_identity;
GRANT SELECT, INSERT, UPDATE, DELETE ON enrollments TO tetsu_identity_lookup;

-- The lookup functions read whether an operator is disabled, and lock its row
-- while they do, so a disable at the same moment never misses what they add.
GRANT SELECT, UPDATE (disabled_at) ON operators TO tetsu_identity_lookup;

-- Starts a session now, as before, but only for an enabled operator: one
-- disabled or unknown raises no_data_found, which Identity reads as invalid.
-- It holds the operator's row until it commits, so a disable waits for it and
-- then ends the session it started.
CREATE OR REPLACE FUNCTION start_session(
    wanted_hash bytea, wanted_realm text, wanted_operator bigint
)
RETURNS void
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz := clock_timestamp();
    absolute interval := CASE wanted_realm
        WHEN 'operator' THEN interval '8 hours'
    END;
    idle interval := CASE wanted_realm
        WHEN 'operator' THEN interval '15 minutes'
    END;
BEGIN
    PERFORM
    FROM identity.operators AS o
    WHERE o.id = wanted_operator AND o.disabled_at IS NULL
    FOR SHARE;
    IF NOT FOUND THEN
        RAISE no_data_found USING MESSAGE = 'no enabled operator';
    END IF;
    DELETE FROM identity.sessions AS s
    WHERE
        s.operator_id = wanted_operator
        AND s.realm = wanted_realm
        AND (at >= s.ends_at OR at >= s.last_seen_at + idle);
    INSERT INTO identity.sessions
        (token_hash, realm, operator_id, started_at, last_seen_at, ends_at)
    VALUES
        (wanted_hash, wanted_realm, wanted_operator, at, at, at + absolute);
END
$$;

-- Issues an operator's enrollment link, replacing any earlier one, and says
-- whether it did: a disabled or unknown operator is issued none.
CREATE FUNCTION issue_enrollment(wanted_hash bytea, wanted_operator bigint)
RETURNS boolean
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH enabled AS (
        SELECT o.id
        FROM identity.operators AS o
        WHERE o.id = wanted_operator AND o.disabled_at IS NULL
        FOR SHARE
    ),

    issued AS (
        INSERT INTO identity.enrollments
            (token_hash, operator_id, issued_at, ends_at)
        SELECT
            wanted_hash,
            enabled.id,
            clock_timestamp(),
            clock_timestamp() + interval '1 hour'
        FROM enabled
        ON CONFLICT (operator_id) DO UPDATE
            SET
                token_hash = excluded.token_hash,
                issued_at = excluded.issued_at,
                ends_at = excluded.ends_at
        RETURNING operator_id
    )

    SELECT EXISTS (SELECT FROM issued)
$$;

-- Disables an operator, and in the same statement ends every session it holds
-- in any realm and its enrollment link (v5.0.0-7.4.2). Says whether it was
-- enabled; disabling one already disabled changes nothing.
CREATE FUNCTION disable_operator(wanted_operator bigint)
RETURNS boolean
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH disabled AS (
        UPDATE identity.operators AS o
        SET disabled_at = clock_timestamp()
        WHERE o.id = wanted_operator AND o.disabled_at IS NULL
        RETURNING o.id
    ),

    sessions AS (
        DELETE FROM identity.sessions AS s
        USING disabled
        WHERE s.operator_id = disabled.id
    ),

    links AS (
        DELETE FROM identity.enrollments AS e
        USING disabled
        WHERE e.operator_id = disabled.id
    )

    SELECT EXISTS (SELECT FROM disabled)
$$;

-- Ends every session one operator holds in a realm, and says how many
-- (v5.0.0-7.4.5).
CREATE FUNCTION end_operator_sessions(wanted_operator bigint, wanted_realm text)
RETURNS bigint
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH ended AS (
        DELETE FROM identity.sessions AS s
        WHERE s.operator_id = wanted_operator AND s.realm = wanted_realm
        RETURNING 1
    )

    SELECT count(*) FROM ended
$$;

-- Ends every session in a realm, and says how many (v5.0.0-7.4.5).
CREATE FUNCTION end_realm_sessions(wanted_realm text)
RETURNS bigint
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH ended AS (
        DELETE FROM identity.sessions AS s
        WHERE s.realm = wanted_realm
        RETURNING 1
    )

    SELECT count(*) FROM ended
$$;

REVOKE EXECUTE ON FUNCTION start_session(bytea, text, bigint),
issue_enrollment(bytea, bigint), disable_operator(bigint),
end_operator_sessions(bigint, text), end_realm_sessions(text) FROM public;
GRANT EXECUTE ON FUNCTION start_session(bytea, text, bigint),
issue_enrollment(bytea, bigint), disable_operator(bigint),
end_operator_sessions(bigint, text), end_realm_sessions(text)
TO tetsu_identity;

GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION start_session(bytea, text, bigint)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION issue_enrollment(bytea, bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION disable_operator(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_operator_sessions(bigint, text)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_realm_sessions(text) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
