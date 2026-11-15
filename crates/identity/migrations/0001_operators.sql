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

CREATE FUNCTION start_session(
    wanted_hash bytea,
    wanted_realm text,
    wanted_operator bigint,
    started timestamptz,
    ends timestamptz
)
RETURNS void
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    INSERT INTO identity.sessions
        (token_hash, realm, operator_id, started_at, last_seen_at, ends_at)
    VALUES (wanted_hash, wanted_realm, wanted_operator, started, started, ends)
$$;

-- Only in the realm asked for, so a token from another realm never passes as
-- this one's (ADR-0040).
CREATE FUNCTION session_for(wanted_hash bytea, wanted_realm text)
RETURNS TABLE (
    realm text,
    operator_id bigint,
    started_at timestamptz,
    last_seen_at timestamptz,
    ends_at timestamptz
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT s.realm, s.operator_id, s.started_at, s.last_seen_at, s.ends_at
    FROM identity.sessions AS s
    WHERE s.token_hash = wanted_hash AND s.realm = wanted_realm
$$;

-- Never moves the last request back, so requests that finish out of order
-- cannot shorten a session.
CREATE FUNCTION touch_session(wanted_hash bytea, seen timestamptz)
RETURNS void
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    UPDATE identity.sessions AS s
    SET last_seen_at = greatest(s.last_seen_at, seen)
    WHERE s.token_hash = wanted_hash
$$;

CREATE FUNCTION end_session(wanted_hash bytea)
RETURNS void
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    DELETE FROM identity.sessions AS s WHERE s.token_hash = wanted_hash
$$;

REVOKE EXECUTE ON FUNCTION start_session(
    bytea, text, bigint, timestamptz, timestamptz
),
session_for(bytea, text), touch_session(bytea, timestamptz),
end_session(bytea) FROM public;
GRANT EXECUTE ON FUNCTION start_session(
    bytea, text, bigint, timestamptz, timestamptz
),
session_for(bytea, text), touch_session(bytea, timestamptz),
end_session(bytea) TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION start_session(bytea, text, bigint, timestamptz, timestamptz)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION session_for(bytea, text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION touch_session(bytea, timestamptz) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_session(bytea) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
