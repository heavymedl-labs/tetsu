-- Operators, who work for Tetsu itself, their sessions, and their enrollment
-- links, and what the operator command does to them (ADR-0040, ADR-0048,
-- ADR-0059). All are global: an operator belongs to no business, and a session
-- or link is found by its token before any business is known.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE TABLE operators (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- No control character or line break either, so no name can start a new
    -- line of output. Unique, so retrying a create finds the first instead of
    -- making a second.
    name text NOT NULL UNIQUE CHECK (
        char_length(name) BETWEEN 1 AND 100
        AND name !~ '[[:cntrl:]\u2028\u2029]'
    ),
    -- WebAuthn's user handle: random, so it says nothing about the operator
    -- (ADR-0040).
    user_handle uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    created_at timestamptz NOT NULL DEFAULT now(),
    -- An operator is disabled rather than removed, so the audit record keeps
    -- a name to point at. A disabled operator starts no session, uses none,
    -- and is issued no enrollment link (ADR-0059).
    disabled_at timestamptz
);

COMMENT ON TABLE operators IS
'tetsu:global: operators work for Tetsu, not a business';

-- A session is kept under its token's SHA-256 hash, never the token
-- (ADR-0047).
CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    -- What a list of a person's sessions shows and ends them by: random, so
    -- it says nothing about the token, whose hash never leaves the database.
    id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    realm text NOT NULL CHECK (realm IN ('operator')),
    operator_id bigint NOT NULL REFERENCES operators (id),
    started_at timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL
)
-- Every signed-in request rewrites last_seen_at, which no index names, so
-- room on the page keeps each rewrite there (ADR-0060).
WITH (fillfactor = 80);

COMMENT ON TABLE sessions IS
'tetsu:global: found by its token before any business is known';

CREATE INDEX sessions_operator_id ON sessions (operator_id);

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

-- An operator's passkeys, two at most (ADR-0057), each as webauthn-rs keeps
-- it: a public key, a signature counter, and flags, never a secret.
CREATE TABLE passkeys (
    credential_id bytea PRIMARY KEY CHECK (
        octet_length(credential_id) >= 16
        AND octet_length(credential_id) <= 1023
    ),
    operator_id bigint NOT NULL REFERENCES operators (id),
    passkey jsonb NOT NULL CHECK (octet_length(passkey::text) <= 8192),
    created_at timestamptz NOT NULL,
    last_used_at timestamptz
);

COMMENT ON TABLE passkeys IS
'tetsu:global: found by its credential before anyone is signed in';

CREATE INDEX passkeys_operator_id ON passkeys (operator_id);

-- A passkey ceremony under way: the state webauthn-rs needs to finish it,
-- under the hash of a token only the browser holds, good once and for five
-- minutes, and never sent to the browser (ADR-0047). An enrollment keeps the
-- link it will redeem, which is used up only when the passkey is stored.
CREATE TABLE ceremonies (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    kind text NOT NULL CHECK (kind IN ('enroll')),
    operator_id bigint NOT NULL REFERENCES operators (id),
    link_hash bytea CHECK (octet_length(link_hash) = 32),
    state jsonb NOT NULL CHECK (octet_length(state::text) <= 16384),
    ends_at timestamptz NOT NULL,
    CHECK (kind <> 'enroll' OR link_hash IS NOT NULL)
);

COMMENT ON TABLE ceremonies IS
'tetsu:global: found by its token before anyone is signed in';

CREATE INDEX ceremonies_operator_id ON ceremonies (operator_id);

-- The runner's default privileges give Identity's role every right on a new
-- table; each is replaced by exactly what is granted below.
REVOKE ALL ON operators, sessions, enrollments, passkeys, ceremonies
FROM tetsu_identity;
REVOKE ALL ON SEQUENCE operators_id_seq FROM tetsu_identity;

-- Identity's role adds and reads operators, and reaches sessions and links
-- only through the functions below, one token's hash a call, so no caller can
-- list them. Those functions read whether an operator is disabled, and lock
-- its row while they do, so a disable at the same moment never misses what
-- they add.
GRANT SELECT (id, name), INSERT (name) ON operators TO tetsu_identity;
GRANT SELECT (
    token_hash, realm, operator_id, started_at, last_seen_at, ends_at
),
INSERT (token_hash, realm, operator_id, started_at, last_seen_at, ends_at),
UPDATE (last_seen_at),
DELETE ON sessions TO tetsu_identity_lookup;
GRANT SELECT, INSERT, DELETE ON enrollments TO tetsu_identity_lookup;
GRANT UPDATE (token_hash, issued_at, ends_at) ON enrollments
TO tetsu_identity_lookup;
GRANT SELECT (id, name, user_handle, created_at, disabled_at),
UPDATE (disabled_at)
ON operators TO tetsu_identity_lookup;
GRANT SELECT (credential_id, operator_id),
INSERT (credential_id, operator_id, passkey, created_at)
ON passkeys TO tetsu_identity_lookup;
GRANT SELECT, INSERT, DELETE ON ceremonies TO tetsu_identity_lookup;

-- Each realm's limits live here, beside the clock that applies them, so no
-- caller can lengthen a session by passing other numbers (ADR-0048). Each
-- function reads the clock once, so neither a server's clock nor a request
-- handled late can stretch a session. A use that waits for another's lock on
-- the same row is judged as of its own start, a wait no longer than the lock
-- timeout; a start reads the clock after its wait, so it is never dated
-- before what held it up.

-- Starts a session now, only for an enabled operator, and removes the
-- operator's spent sessions in the realm, so they do not pile up. One
-- disabled or unknown raises TD001, Identity's own code, which it reads as
-- invalid. It holds the operator's row until it commits, so a disable waits
-- for it and then ends the session it started.
CREATE FUNCTION start_session(
    wanted_hash bytea, wanted_realm text, wanted_operator bigint
)
RETURNS void
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
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
        RAISE EXCEPTION 'no enabled operator' USING ERRCODE = 'TD001';
    END IF;
    -- After any wait for the operator's row, so the session starts when it
    -- was stored, never before an end of sessions that held it up.
    at := clock_timestamp();
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

-- Records a request on the live session a token's hash names in a realm, and
-- returns it; a session past either limit is removed instead, and none is
-- returned. The clock is read once. The update waits for any request holding
-- the row and checks it again as that request left it, so requests at the
-- same moment never end a live session; only a session the update did not
-- find live is removed, and only if it is spent, so none is revived, and the
-- last request never moves back. Only the realm asked for counts, so a token
-- from another realm never passes as this one's (ADR-0040, ADR-0048).
-- A session of a disabled operator is never used, and is removed when
-- presented: a disable ends its operator's sessions, and this keeps any it
-- could miss from acting for them.
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
    FROM identity.operators AS o
    WHERE
        s.token_hash = wanted_hash
        AND s.realm = wanted_realm
        AND at < s.ends_at
        AND at < s.last_seen_at + idle
        AND o.id = s.operator_id
        AND o.disabled_at IS NULL
    RETURNING s.realm, s.operator_id, s.started_at, s.last_seen_at, s.ends_at;
    IF NOT FOUND THEN
        DELETE FROM identity.sessions AS s
        USING identity.operators AS o
        WHERE
            s.token_hash = wanted_hash
            AND s.realm = wanted_realm
            AND o.id = s.operator_id
            AND (
                at >= s.ends_at
                OR at >= s.last_seen_at + idle
                OR o.disabled_at IS NOT NULL
            );
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

-- Issues an operator's enrollment link, replacing any earlier one, and says
-- what it did: issued; disabled, for an operator issued none; enrolled, for
-- one who has a passkey already and adds another only after signing in
-- (v5.0.0-6.4.1); or null for one that does not exist. It takes the
-- operator's row, so a passkey being stored for them is seen, which holds only
-- under read committed, so it refuses any other level with TD002. It reads the
-- clock once, so every link lasts exactly an hour.
CREATE FUNCTION issue_enrollment(wanted_hash bytea, wanted_operator bigint)
RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    was timestamptz;
BEGIN
    IF current_setting('transaction_isolation') <> 'read committed' THEN
        RAISE EXCEPTION 'issuing a link needs read committed'
        USING ERRCODE = 'TD002';
    END IF;
    SELECT o.disabled_at INTO was
    FROM identity.operators AS o
    WHERE o.id = wanted_operator
    FOR SHARE;
    IF NOT FOUND THEN
        RETURN null;
    ELSIF was IS NOT NULL THEN
        RETURN 'disabled';
    ELSIF EXISTS (
        SELECT FROM identity.passkeys AS p WHERE p.operator_id = wanted_operator
    ) THEN
        RETURN 'enrolled';
    END IF;
    at := clock_timestamp();
    INSERT INTO identity.enrollments
        (token_hash, operator_id, issued_at, ends_at)
    VALUES (wanted_hash, wanted_operator, at, at + interval '1 hour')
    ON CONFLICT (operator_id) DO UPDATE
        SET
            token_hash = excluded.token_hash,
            issued_at = excluded.issued_at,
            ends_at = excluded.ends_at;
    RETURN 'issued';
END
$$;

-- Every operator, oldest first, with how many passkeys each holds and how many
-- minutes their open enrollment link has left, if one is open. Only counts
-- and times leave: never a credential or a token's hash. One reading of the
-- clock, the statement's, serves every row.
CREATE FUNCTION list_operators()
RETURNS TABLE (
    id bigint,
    name text,
    created_at timestamptz,
    disabled_at timestamptz,
    passkeys bigint,
    link_minutes bigint
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT
        o.id,
        o.name,
        o.created_at,
        o.disabled_at,
        (
            SELECT count(*) FROM identity.passkeys AS p
            WHERE p.operator_id = o.id
        ) AS passkeys,
        (
            SELECT
                ceil(
                    extract(EPOCH FROM e.ends_at - statement_timestamp()) / 60
                )::bigint
            FROM identity.enrollments AS e
            WHERE e.operator_id = o.id AND e.ends_at > statement_timestamp()
        ) AS link_minutes
    FROM identity.operators AS o
    ORDER BY o.id
$$;

-- The operator an enrollment link names, while the link is open, the operator
-- enabled, and no passkey theirs yet, with the user handle and name a
-- registration needs; nothing otherwise. Reading a link uses nothing up.
CREATE FUNCTION enrollee(wanted_link bytea)
RETURNS TABLE (operator_id bigint, user_handle bytea, name text)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT
        o.id,
        uuid_send(o.user_handle),
        o.name
    FROM identity.enrollments AS e
    INNER JOIN identity.operators AS o ON e.operator_id = o.id
    WHERE
        e.token_hash = wanted_link
        AND e.ends_at > statement_timestamp()
        AND o.disabled_at IS NULL
        AND NOT EXISTS (
            SELECT FROM identity.passkeys AS p WHERE p.operator_id = o.id
        )
$$;

-- Keeps an enrollment ceremony's state under its token's hash, and says
-- whether it did: only while the link is open, its operator enabled, and no
-- passkey theirs, checked here rather than trusted from the caller. A
-- ceremony lasts ten minutes, longer than the five a browser gives its
-- authenticator, but never past its link. A link holds one ceremony at most:
-- a newer one replaces it, as a new link replaces the last, so beginning again
-- and again stores nothing more. The operator's ceremonies that have ended go
-- too, through the index on their operator.
CREATE FUNCTION begin_enrollment(
    wanted_hash bytea,
    wanted_operator bigint,
    wanted_link bytea,
    wanted_state jsonb
)
RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz := clock_timestamp();
BEGIN
    DELETE FROM identity.ceremonies AS c
    WHERE
        c.operator_id = wanted_operator
        AND (c.ends_at <= at OR c.link_hash = wanted_link);
    INSERT INTO identity.ceremonies
        (token_hash, kind, operator_id, link_hash, state, ends_at)
    SELECT
        wanted_hash,
        'enroll',
        e.operator_id,
        e.token_hash,
        wanted_state,
        least(at + interval '10 minutes', e.ends_at)
    FROM identity.enrollments AS e
    INNER JOIN identity.operators AS o ON e.operator_id = o.id
    WHERE
        e.token_hash = wanted_link
        AND e.operator_id = wanted_operator
        AND e.ends_at > at
        AND o.disabled_at IS NULL
        AND NOT EXISTS (
            SELECT FROM identity.passkeys AS p WHERE p.operator_id = o.id
        );
    RETURN FOUND;
END
$$;

-- Takes a ceremony of a kind by its token's hash, once: it is removed whether
-- or not it is still open, and returned only if it is.
CREATE FUNCTION take_ceremony(wanted_hash bytea, wanted_kind text)
RETURNS TABLE (operator_id bigint, link_hash bytea, state text)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH taken AS (
        DELETE FROM identity.ceremonies AS c
        WHERE c.token_hash = wanted_hash AND c.kind = wanted_kind
        RETURNING c.operator_id, c.link_hash, c.state, c.ends_at
    )

    SELECT
        taken.operator_id,
        taken.link_hash,
        taken.state::text
    FROM taken
    WHERE taken.ends_at > clock_timestamp()
$$;

-- Stores an operator's first passkey and uses up the link that let them, in
-- one transaction, and says what came of it: redeemed; disabled; enrolled,
-- for an operator who has a passkey already, as when two tabs finish one
-- link; expired, for a link that ended or was replaced; taken, for a
-- credential another passkey has, even one stored a moment before; or null
-- for an operator who does not exist. It takes the operator's row first, so a
-- link being issued, a disable, or another tab's passkey waits, and what it
-- checks stays true until it commits, under read committed only, so it
-- refuses any other level with TD002.
CREATE FUNCTION redeem_enrollment(
    wanted_link bytea,
    wanted_operator bigint,
    wanted_credential bytea,
    wanted_passkey jsonb
)
RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    was timestamptz;
BEGIN
    IF current_setting('transaction_isolation') <> 'read committed' THEN
        RAISE EXCEPTION 'storing a passkey needs read committed'
        USING ERRCODE = 'TD002';
    END IF;
    SELECT o.disabled_at INTO was
    FROM identity.operators AS o
    WHERE o.id = wanted_operator
    FOR NO KEY UPDATE;
    IF NOT FOUND THEN
        RETURN null;
    ELSIF was IS NOT NULL THEN
        RETURN 'disabled';
    ELSIF EXISTS (
        SELECT FROM identity.passkeys AS p WHERE p.operator_id = wanted_operator
    ) THEN
        RETURN 'enrolled';
    END IF;
    -- A credential another operator's passkey has, however recently, undoes
    -- the link's use and is a refusal, recorded like any other.
    BEGIN
        DELETE FROM identity.enrollments AS e
        WHERE
            e.token_hash = wanted_link
            AND e.operator_id = wanted_operator
            AND e.ends_at > clock_timestamp();
        IF NOT FOUND THEN
            RETURN 'expired';
        END IF;
        INSERT INTO identity.passkeys
            (credential_id, operator_id, passkey, created_at)
        VALUES (
            wanted_credential, wanted_operator, wanted_passkey, clock_timestamp()
        );
    EXCEPTION WHEN unique_violation THEN
        RETURN 'taken';
    END;
    RETURN 'redeemed';
END
$$;

-- Disables an operator, then ends every session it holds in any realm, its
-- enrollment link, and any passkey ceremony under way (v5.0.0-7.4.2), and
-- says whether it acted and how many live sessions and links it ended, spent
-- ones going too: done is false for an operator already
-- disabled, and null for one that does not exist. Each step is its own
-- statement, so the deletes see what a sign-in or link committed while the
-- lock waited on the operator's row. One statement would not, and nor would
-- repeatable read, which it refuses with TD002, as do the functions that end
-- sessions.
CREATE FUNCTION disable_operator(wanted_operator bigint)
RETURNS TABLE (done boolean, sessions_ended bigint, links_ended bigint)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    was timestamptz;
BEGIN
    IF current_setting('transaction_isolation') <> 'read committed' THEN
        RAISE EXCEPTION 'disabling needs read committed' USING ERRCODE = 'TD002';
    END IF;
    sessions_ended := 0;
    links_ended := 0;
    -- The row first, waiting out any enable, so its answer is the state it
    -- leaves, never "already disabled" over an enable about to commit.
    SELECT o.disabled_at INTO was
    FROM identity.operators AS o
    WHERE o.id = wanted_operator
    FOR NO KEY UPDATE;
    IF NOT FOUND OR was IS NOT NULL THEN
        done := CASE WHEN FOUND THEN false END;
        RETURN NEXT;
        RETURN;
    END IF;
    at := clock_timestamp();
    UPDATE identity.operators AS o SET disabled_at = at
    WHERE o.id = wanted_operator;
    WITH gone AS (
        DELETE FROM identity.sessions AS s
        WHERE s.operator_id = wanted_operator
        RETURNING
            at < s.ends_at
            AND at < s.last_seen_at + CASE s.realm
                WHEN 'operator' THEN interval '15 minutes'
            END AS live
    )

    SELECT count(*) FILTER (WHERE gone.live) INTO sessions_ended FROM gone;
    WITH gone AS (
        DELETE FROM identity.enrollments AS e
        WHERE e.operator_id = wanted_operator
        RETURNING at < e.ends_at AS live
    )

    SELECT count(*) FILTER (WHERE gone.live) INTO links_ended FROM gone;
    DELETE FROM identity.ceremonies AS c WHERE c.operator_id = wanted_operator;
    done := true;
    RETURN NEXT;
END
$$;

-- Enables a disabled operator again, and says whether it acted: false for an
-- operator already enabled, null for one that does not exist. Like a disable,
-- it takes the row first, so its answer is the state it leaves. Their
-- sessions stay ended, and they sign in afresh.
CREATE FUNCTION enable_operator(wanted_operator bigint)
RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    was timestamptz;
BEGIN
    SELECT o.disabled_at INTO was
    FROM identity.operators AS o
    WHERE o.id = wanted_operator
    FOR NO KEY UPDATE;
    IF NOT FOUND THEN
        RETURN null;
    END IF;
    UPDATE identity.operators AS o SET disabled_at = null
    WHERE o.id = wanted_operator;
    RETURN was IS NOT NULL;
END
$$;

-- Ends every session one operator holds in a realm, or every operator's when
-- none is named, and says how many were live, spent ones going too, or null
-- for an operator who does not exist (v5.0.0-7.4.5). It first takes the
-- operators' rows, so a sign-in that holds one finishes before the delete,
-- which then ends it too. Operators are few, so taking every row holds little.
CREATE FUNCTION end_sessions(wanted_realm text, wanted_operator bigint)
RETURNS bigint
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    ended bigint;
BEGIN
    IF current_setting('transaction_isolation') <> 'read committed' THEN
        RAISE EXCEPTION 'ending sessions needs read committed'
        USING ERRCODE = 'TD002';
    END IF;
    PERFORM
    FROM identity.operators AS o
    WHERE wanted_operator IS NULL OR o.id = wanted_operator
    ORDER BY o.id
    FOR NO KEY UPDATE;
    IF NOT FOUND AND wanted_operator IS NOT NULL THEN
        RETURN null;
    END IF;
    at := clock_timestamp();
    WITH gone AS (
        DELETE FROM identity.sessions AS s
        WHERE
            s.realm = wanted_realm
            AND (wanted_operator IS NULL OR s.operator_id = wanted_operator)
        RETURNING
            at < s.ends_at
            AND at < s.last_seen_at + CASE s.realm
                WHEN 'operator' THEN interval '15 minutes'
            END AS live
    )

    SELECT count(*) FILTER (WHERE gone.live) INTO ended FROM gone;
    RETURN ended;
END
$$;

-- No routine here is anyone's by default; Identity's role may run exactly the
-- lookups it calls, so a helper added later is no one's until granted.
REVOKE EXECUTE ON ALL ROUTINES IN SCHEMA identity FROM public;
GRANT EXECUTE ON FUNCTION start_session(bytea, text, bigint),
use_session(bytea, text), end_session(bytea, text),
issue_enrollment(bytea, bigint), list_operators(), enrollee(bytea),
begin_enrollment(bytea, bigint, bytea, jsonb), take_ceremony(bytea, text),
redeem_enrollment(bytea, bigint, bytea, jsonb), disable_operator(bigint),
enable_operator(bigint), end_sessions(text, bigint) TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION start_session(bytea, text, bigint)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION use_session(bytea, text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_session(bytea, text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION issue_enrollment(bytea, bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION list_operators() OWNER TO tetsu_identity_lookup;
ALTER FUNCTION enrollee(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION begin_enrollment(bytea, bigint, bytea, jsonb)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION take_ceremony(bytea, text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION redeem_enrollment(bytea, bigint, bytea, jsonb)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION disable_operator(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION enable_operator(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_sessions(text, bigint) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
