-- Admins, who work for Tetsu itself, their sessions, and their join
-- links, and what the admin command does to them (ADR-0040, ADR-0048,
-- ADR-0059). All are global: an admin belongs to no business, and a session
-- or link is found by its token before any business is known.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE TABLE admins (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- No control character or line break either, so no name can start a new
    -- line of output. Unique, so retrying a create finds the first instead of
    -- making a second.
    name text NOT NULL UNIQUE CHECK (
        char_length(name) BETWEEN 1 AND 100
        AND name !~ '[[:cntrl:]\u2028\u2029]'
    ),
    -- WebAuthn's user handle: random, so it says nothing about the admin
    -- (ADR-0040).
    user_handle uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    created_at timestamptz NOT NULL DEFAULT now(),
    -- An admin is disabled rather than removed, so the audit record keeps
    -- a name to point at. A disabled admin starts no session, uses none,
    -- and is issued no invite link (ADR-0059).
    disabled_at timestamptz
);

COMMENT ON TABLE admins IS
'tetsu:global: admins work for Tetsu, not a business';

-- A session is kept under its token's SHA-256 hash, never the token
-- (ADR-0047).
CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    -- What a list of a person's sessions shows and ends them by: random, so
    -- it says nothing about the token, whose hash never leaves the database.
    id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    realm text NOT NULL CHECK (realm IN ('admin')),
    admin_id bigint NOT NULL REFERENCES admins (id),
    started_at timestamptz NOT NULL,
    -- When a passkey last proved the person in this session: at its start for
    -- one a login began, and never for one joining began, which rests on the
    -- invite link alone (ADR-0069). Only a proof within ten minutes is fresh.
    proven_at timestamptz,
    last_seen_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL
)
-- Every logged-in request rewrites last_seen_at, which no index names, so
-- room on the page keeps each rewrite there (ADR-0060).
WITH (fillfactor = 80);

COMMENT ON TABLE sessions IS
'tetsu:global: found by its token before any business is known';

CREATE INDEX sessions_admin_id ON sessions (admin_id);

-- Each admin's one open invite link, kept under its token's SHA-256
-- hash, never the token, and good for an hour. A new link replaces the last.
CREATE TABLE invites (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    admin_id bigint NOT NULL UNIQUE REFERENCES admins (id),
    issued_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL
);

COMMENT ON TABLE invites IS
'tetsu:global: found by its token before anyone is logged in';

-- An admin's passkeys, two at most (ADR-0057), each as webauthn-rs keeps
-- it: a public key, a signature counter, and flags, never a secret.
CREATE TABLE passkeys (
    credential_id bytea PRIMARY KEY CHECK (
        octet_length(credential_id) >= 16
        AND octet_length(credential_id) <= 1023
    ),
    admin_id bigint NOT NULL REFERENCES admins (id),
    -- Joining and adding store no passkey over 8 KiB; the table keeps a
    -- little more, so a counter that gains digits never fails a stored one.
    passkey jsonb NOT NULL CHECK (octet_length(passkey::text) <= 8320),
    -- How the browser said it can reach the passkey, as a hint for the next
    -- ceremony: the browser's word, never trusted. Only the names WebAuthn
    -- gives, separated by spaces; Identity writes each once, sorted, so all
    -- five fit in 27.
    transports text NOT NULL DEFAULT '' CHECK (
        char_length(transports) <= 27
        AND transports
        ~ '^((ble|hybrid|internal|nfc|usb)( (ble|hybrid|internal|nfc|usb))*)?$'
    ),
    created_at timestamptz NOT NULL,
    last_used_at timestamptz
);

COMMENT ON TABLE passkeys IS
'tetsu:global: found by its credential before anyone is logged in';

CREATE INDEX passkeys_admin_id ON passkeys (admin_id);

-- A passkey ceremony under way: the state webauthn-rs needs to finish it,
-- under the hash of a token only the browser holds, good once and for ten
-- minutes, and never sent to the browser (ADR-0047). A join keeps the
-- link it will redeem, which is used up only when the passkey is stored.
CREATE TABLE ceremonies (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    kind text NOT NULL CHECK (kind IN ('join', 'login', 'add')),
    -- A join knows its admin and link, and adding a passkey its
    -- admin; a login knows neither until the browser answers.
    admin_id bigint REFERENCES admins (id),
    link_hash bytea CHECK (octet_length(link_hash) = 32),
    -- The session an addition began from, which alone may finish it; the
    -- addition ends with it.
    session_hash bytea CHECK (octet_length(session_hash) = 32)
    REFERENCES sessions (token_hash) ON DELETE CASCADE,
    -- An addition taken to be verified, which only storing its passkey may
    -- then use up.
    taken boolean NOT NULL DEFAULT FALSE,
    state jsonb NOT NULL CHECK (octet_length(state::text) <= 16384),
    ends_at timestamptz NOT NULL,
    CHECK ((kind IN ('join', 'add')) = (admin_id IS NOT NULL)),
    CHECK ((kind = 'join') = (link_hash IS NOT NULL)),
    CHECK ((kind = 'add') = (session_hash IS NOT NULL)),
    CHECK (kind = 'add' OR NOT taken)
);

COMMENT ON TABLE ceremonies IS
'tetsu:global: found by its token before anyone is logged in';

CREATE INDEX ceremonies_admin_id ON ceremonies (admin_id);

-- A link holds one ceremony at most, however many begin at once.
CREATE UNIQUE INDEX ceremonies_link_hash ON ceremonies (link_hash);

-- Anyone may begin a login, so ended ceremonies are swept by when they end.
CREATE INDEX ceremonies_ends_at ON ceremonies (ends_at);

-- An admin adds one passkey at a time, however many begin at once.
CREATE UNIQUE INDEX ceremonies_addition ON ceremonies (admin_id)
WHERE kind = 'add';

-- Ending a session finds the addition it began.
CREATE INDEX ceremonies_session_hash ON ceremonies (session_hash)
WHERE session_hash IS NOT NULL;

-- The runner's default privileges give Identity's role every right on a new
-- table; each is replaced by exactly what is granted below.
REVOKE ALL ON admins, sessions, invites, passkeys, ceremonies
FROM tetsu_identity;
REVOKE ALL ON SEQUENCE admins_id_seq FROM tetsu_identity;

-- Identity's role adds and reads admins, and reaches sessions and links
-- only through the functions below, one token's hash a call, so no caller can
-- list them. Those functions read whether an admin is disabled, and lock
-- its row while they do, so a disable at the same moment never misses what
-- they add.
GRANT SELECT (id, name), INSERT (name) ON admins TO tetsu_identity;
-- The public ID too, which audit records name a session by, never its hash.
GRANT SELECT (
    token_hash, id, realm, admin_id, started_at, proven_at, last_seen_at,
    ends_at
),
INSERT (
    token_hash, realm, admin_id, started_at, proven_at, last_seen_at,
    ends_at
),
UPDATE (last_seen_at),
DELETE ON sessions TO tetsu_identity_lookup;
-- Replacing a link reads every column its upsert sets, issued_at included.
GRANT SELECT, INSERT, DELETE ON invites TO tetsu_identity_lookup;
GRANT UPDATE (token_hash, issued_at, ends_at) ON invites
TO tetsu_identity_lookup;
GRANT SELECT (id, name, user_handle, created_at, disabled_at),
UPDATE (disabled_at)
ON admins TO tetsu_identity_lookup;
GRANT SELECT (credential_id, admin_id, passkey, transports),
INSERT (credential_id, admin_id, passkey, transports, created_at),
UPDATE (passkey, last_used_at),
DELETE
ON passkeys TO tetsu_identity_lookup;
GRANT SELECT, DELETE ON ceremonies TO tetsu_identity_lookup;
GRANT INSERT (
    token_hash, kind, admin_id, link_hash, session_hash, state, ends_at
) ON ceremonies TO tetsu_identity_lookup;
GRANT UPDATE (token_hash, session_hash, taken, state, ends_at) ON ceremonies
TO tetsu_identity_lookup;

-- Each realm's limits live here, beside the clock that applies them, so no
-- caller can lengthen a session by passing other numbers (ADR-0048). Each
-- function reads the clock once, so neither a server's clock nor a request
-- handled late can stretch a session. A use that waits for another's lock on
-- the same row is judged as of its own start, a wait no longer than the lock
-- timeout; a start reads the clock after its wait, so it is never dated
-- before what held it up.

-- Starts a session now, only for an enabled admin, and removes the
-- admin's spent sessions in the realm, so they do not pile up. Only a login
-- and a redemption call it, each holding an enabled admin's row, so one
-- disabled or unknown is a fault, raised as TD001, Identity's own code. A
-- session is proven only when a passkey login starts it (ADR-0069). It holds
-- the admin's row until it commits, so a disable waits for it and then ends
-- the session it started.
CREATE FUNCTION start_session(
    wanted_hash bytea, wanted_realm text, wanted_admin bigint,
    wanted_proven boolean
)
RETURNS void
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    absolute interval := CASE wanted_realm
        WHEN 'admin' THEN interval '8 hours'
    END;
    idle interval := CASE wanted_realm
        WHEN 'admin' THEN interval '15 minutes'
    END;
BEGIN
    PERFORM
    FROM identity.admins AS o
    WHERE o.id = wanted_admin AND o.disabled_at IS NULL
    FOR SHARE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'no enabled admin' USING ERRCODE = 'TD001';
    END IF;
    -- After any wait for the admin's row, so the session starts when it
    -- was stored, never before an end of sessions that held it up.
    at := clock_timestamp();
    DELETE FROM identity.sessions AS s
    WHERE
        s.admin_id = wanted_admin
        AND s.realm = wanted_realm
        AND (at >= s.ends_at OR at >= s.last_seen_at + idle);
    INSERT INTO identity.sessions (
        token_hash, realm, admin_id, started_at, proven_at, last_seen_at,
        ends_at
    )
    VALUES (
        wanted_hash, wanted_realm, wanted_admin, at,
        CASE WHEN wanted_proven THEN at END, at, at + absolute
    );
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
-- A session of a disabled admin is never used, and is removed when
-- presented: a disable ends its admin's sessions, and this keeps any it
-- could miss from acting for them.
CREATE FUNCTION use_session(wanted_hash bytea, wanted_realm text)
RETURNS TABLE (
    realm text,
    admin_id bigint,
    started_at timestamptz,
    proven_at timestamptz,
    last_seen_at timestamptz,
    ends_at timestamptz
)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz := clock_timestamp();
    idle interval := CASE wanted_realm
        WHEN 'admin' THEN interval '15 minutes'
    END;
BEGIN
    RETURN QUERY
    UPDATE identity.sessions AS s
    SET last_seen_at = greatest(s.last_seen_at, at)
    FROM identity.admins AS o
    WHERE
        s.token_hash = wanted_hash
        AND s.realm = wanted_realm
        AND at < s.ends_at
        AND at < s.last_seen_at + idle
        AND o.id = s.admin_id
        AND o.disabled_at IS NULL
    RETURNING
        s.realm, s.admin_id, s.started_at, s.proven_at, s.last_seen_at,
        s.ends_at;
    IF NOT FOUND THEN
        DELETE FROM identity.sessions AS s
        USING identity.admins AS o
        WHERE
            s.token_hash = wanted_hash
            AND s.realm = wanted_realm
            AND o.id = s.admin_id
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
            CASE wanted_realm WHEN 'admin' THEN interval '15 minutes' END
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

-- Refuses any isolation level but read committed with TD002, naming what was
-- refused. Every function that waits on an admin's row and then checks what
-- changed calls it first: under another level, a row committed while it waited
-- escapes its checks. Only Identity's functions call it; no other role may.
CREATE FUNCTION require_read_committed(what text)
RETURNS void
LANGUAGE plpgsql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
BEGIN
    IF current_setting('transaction_isolation') <> 'read committed' THEN
        RAISE EXCEPTION '% needs read committed', what USING ERRCODE = 'TD002';
    END IF;
END
$$;

-- Where an admin stands for join, after taking their row for update,
-- since both callers change what the admin holds: open, disabled, joined
-- for one who holds a passkey already, or null for one who does not exist.
-- What it checks stays true until the caller commits, under read committed
-- only, so it refuses any other level with TD002. Only the functions below
-- call it; no other role may.
CREATE FUNCTION joining(wanted_admin bigint)
RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    was timestamptz;
BEGIN
    PERFORM identity.require_read_committed('joining');
    SELECT o.disabled_at INTO was
    FROM identity.admins AS o
    WHERE o.id = wanted_admin
    FOR NO KEY UPDATE;
    IF NOT FOUND THEN
        RETURN null;
    ELSIF was IS NOT NULL THEN
        RETURN 'disabled';
    ELSIF EXISTS (
        SELECT FROM identity.passkeys AS p WHERE p.admin_id = wanted_admin
    ) THEN
        RETURN 'joined';
    END IF;
    RETURN 'open';
END
$$;

-- Issues an admin's invite link, replacing any earlier one, and says
-- what it did: issued; disabled, for an admin issued none; joined, for
-- one who has a passkey already and adds another only after logging in
-- (v5.0.0-6.4.1); or null for one that does not exist. It takes the
-- admin's row for update, since it replaces what they hold, so a passkey
-- being stored for them is seen, and a ceremony being begun from the old link
-- finishes first and then ends with it. That holds only under read committed,
-- so it refuses any other level with TD002. It reads the clock once, so every
-- link lasts exactly an hour.
CREATE FUNCTION issue_invite(wanted_hash bytea, wanted_admin bigint)
RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    standing text;
BEGIN
    standing := identity.joining(wanted_admin);
    IF standing IS DISTINCT FROM 'open' THEN
        RETURN standing;
    END IF;
    at := clock_timestamp();
    -- A ceremony begun from the link this one replaces ends with it.
    DELETE FROM identity.ceremonies AS c
    WHERE c.admin_id = wanted_admin AND c.kind = 'join';
    INSERT INTO identity.invites
        (token_hash, admin_id, issued_at, ends_at)
    VALUES (wanted_hash, wanted_admin, at, at + interval '1 hour')
    ON CONFLICT (admin_id) DO UPDATE
        SET
            token_hash = excluded.token_hash,
            issued_at = excluded.issued_at,
            ends_at = excluded.ends_at;
    RETURN 'issued';
END
$$;

-- Every admin, oldest first, with how many passkeys each holds and how many
-- minutes their open invite link has left, if one is open. Only counts
-- and times leave: never a credential or a token's hash. One reading of the
-- clock, the statement's, serves every row.
CREATE FUNCTION list_admins()
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
            WHERE p.admin_id = o.id
        ) AS passkeys,
        (
            SELECT
                ceil(
                    extract(EPOCH FROM e.ends_at - statement_timestamp()) / 60
                )::bigint
            FROM identity.invites AS e
            WHERE e.admin_id = o.id AND e.ends_at > statement_timestamp()
        ) AS link_minutes
    FROM identity.admins AS o
    ORDER BY o.id
$$;

-- The admin an invite link names, while the link is open, the admin
-- enabled, and no passkey theirs yet, with the user handle and name a
-- registration needs; nothing otherwise. Reading a link uses nothing up.
CREATE FUNCTION invitee(wanted_link bytea)
RETURNS TABLE (admin_id bigint, user_handle bytea, name text)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT
        o.id,
        uuid_send(o.user_handle),
        o.name
    FROM identity.invites AS e
    INNER JOIN identity.admins AS o ON e.admin_id = o.id
    WHERE
        e.token_hash = wanted_link
        AND e.ends_at > statement_timestamp()
        AND o.disabled_at IS NULL
        AND NOT EXISTS (
            SELECT FROM identity.passkeys AS p WHERE p.admin_id = o.id
        )
$$;

-- Takes an enabled admin's row in share mode, as a ceremony that will add
-- a passkey for them begins, so a disable or a passkey being stored waits for
-- it or is seen by it, sweeps the admin's ceremonies that have ended, and
-- says when, by the clock read after any wait; null for an admin not
-- enabled. Under read committed only (TD002). Only the
-- functions that begin such ceremonies call it; no other role may.
CREATE FUNCTION holding(wanted_admin bigint)
RETURNS timestamptz
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
BEGIN
    PERFORM identity.require_read_committed('beginning a ceremony');
    PERFORM
    FROM identity.admins AS o
    WHERE o.id = wanted_admin AND o.disabled_at IS NULL
    FOR SHARE;
    IF NOT FOUND THEN
        RETURN null;
    END IF;
    at := clock_timestamp();
    DELETE FROM identity.ceremonies AS c
    WHERE c.admin_id = wanted_admin AND c.ends_at <= at;
    RETURN at;
END
$$;

-- Keeps a join ceremony's state under its token's hash, and says
-- whether it did: only while the link is open, its admin enabled, and no
-- passkey theirs, checked here rather than trusted from the caller. It takes
-- the admin's row first, so a disable or a passkey being stored waits for
-- it or is seen by it, which holds only under read committed (TD002), and
-- reads the clock after any wait. A ceremony lasts ten minutes, longer than
-- the five a browser gives its authenticator, but never past its link. A link
-- holds one ceremony at most, by a unique index: a newer one replaces it, as a
-- new link replaces the last, so beginning again and again stores nothing
-- more. The admin's ceremonies that have ended go too.
CREATE FUNCTION begin_join(
    wanted_hash bytea,
    wanted_admin bigint,
    wanted_link bytea,
    wanted_state jsonb
)
RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
BEGIN
    at := identity.holding(wanted_admin);
    IF at IS NULL THEN
        RETURN false;
    END IF;
    INSERT INTO identity.ceremonies
        (token_hash, kind, admin_id, link_hash, state, ends_at)
    SELECT
        wanted_hash,
        'join',
        e.admin_id,
        e.token_hash,
        wanted_state,
        least(at + interval '10 minutes', e.ends_at)
    FROM identity.invites AS e
    WHERE
        e.token_hash = wanted_link
        AND e.admin_id = wanted_admin
        AND e.ends_at > at
        AND NOT EXISTS (
            SELECT FROM identity.passkeys AS p
            WHERE p.admin_id = wanted_admin
        )
    ON CONFLICT (link_hash) DO UPDATE
        SET
            token_hash = excluded.token_hash,
            state = excluded.state,
            ends_at = excluded.ends_at;
    RETURN FOUND;
END
$$;

-- Keeps a login ceremony's state for ten minutes under its token's hash.
-- Anyone may begin one, so until step 5's rate limits two bounds keep the
-- table small and never refuse a begin (ADR-0048, v5.0.0-6.3.1): each begin
-- sweeps up to a hundred ended ceremonies, and past ten thousand open logins
-- it ends the oldest. A flood of begins can still end a real ceremony before
-- its person answers, until those rate limits. Both skip rows another
-- transaction holds, so begins never wait on each other, under read committed
-- only (TD002), where a skipped row is never a serialisation failure.
CREATE FUNCTION begin_login(wanted_hash bytea, wanted_state jsonb)
RETURNS void
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz := clock_timestamp();
    over bigint;
BEGIN
    PERFORM identity.require_read_committed('beginning a login');
    DELETE FROM identity.ceremonies AS c
    WHERE c.ctid = any(array(
        SELECT ended.ctid
        FROM identity.ceremonies AS ended
        WHERE ended.ends_at <= at
        ORDER BY ended.ends_at
        LIMIT 100
        FOR UPDATE SKIP LOCKED
    ));
    SELECT count(*) - 9999 INTO over
    FROM identity.ceremonies AS c
    WHERE c.kind = 'login';
    DELETE FROM identity.ceremonies AS c
    WHERE c.ctid = any(array(
        SELECT oldest.ctid
        FROM identity.ceremonies AS oldest
        WHERE oldest.kind = 'login'
        ORDER BY oldest.ends_at
        LIMIT greatest(over, 0)
        FOR UPDATE SKIP LOCKED
    ));
    INSERT INTO identity.ceremonies (token_hash, kind, state, ends_at)
    VALUES (wanted_hash, 'login', wanted_state, at + interval '10 minutes');
END
$$;

-- The passkey a credential ID names, with its admin, their user handle,
-- and whether they are disabled, so a refused login names whose it was;
-- nothing for a credential no admin holds. Reading it changes nothing.
CREATE FUNCTION signer(wanted_credential bytea)
RETURNS TABLE (
    admin_id bigint, user_handle bytea, passkey text, disabled boolean
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT
        o.id,
        uuid_send(o.user_handle),
        p.passkey::text,
        o.disabled_at IS NOT NULL
    FROM identity.passkeys AS p
    INNER JOIN identity.admins AS o ON p.admin_id = o.id
    WHERE p.credential_id = wanted_credential
$$;

-- Logs an admin in with a passkey a login verified, and says what came of
-- it, with the new session's public ID: logged_in; disabled, for an admin
-- disabled since the passkey was read; raced, for a passkey another login
-- changed meanwhile, whose counter this one must not overwrite; or null for a
-- passkey that no longer exists or is no longer that admin's. It takes the
-- admin's row as a login does, so a disable waits for it and then ends the
-- session it starts, ends the session the browser presented, stores the
-- passkey's new state only if it is still the one verified against, and starts
-- the session, all in one transaction, under read committed only (TD002). The
-- database cannot check a signature, so it refuses (TD003) a new state that
-- changes more than a login may: the counter, never backwards, the backup
-- state, and backup eligibility, only from false to true; and it stores one
-- only over the very text it was read as, so no number is spelt anew. A login
-- cannot rewrite a passkey into one webauthn-rs cannot read. It is not a guard
-- against Identity's role, which can start a session here and has other ways to
-- replace a passkey; a caller that passes it a token hash already used, of the
-- wrong size, or null fails with that database error.
CREATE FUNCTION login(
    wanted_credential bytea,
    wanted_admin bigint,
    wanted_old jsonb,
    wanted_new jsonb,
    wanted_session bytea,
    wanted_presented bytea
)
RETURNS TABLE (outcome text, session_id text)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    who bigint;
    was timestamptz;
BEGIN
    PERFORM identity.require_read_committed('logging in');
    -- Each moved field must be there on both sides, a counter a whole number
    -- an authenticator could send and the flags true or false, checked before
    -- any of them is compared; everything else must be the same text, not
    -- only the same value, so no number is spelt anew.
    IF NOT coalesce(CASE
        WHEN
            (wanted_new #> '{cred,counter}')::text ~ '^[0-9]{1,10}$'
            AND (wanted_old #> '{cred,counter}')::text ~ '^[0-9]{1,10}$'
            AND jsonb_typeof(wanted_new #> '{cred,backup_state}') = 'boolean'
            AND jsonb_typeof(wanted_old #> '{cred,backup_state}') = 'boolean'
            AND jsonb_typeof(wanted_new #> '{cred,backup_eligible}') = 'boolean'
            AND jsonb_typeof(wanted_old #> '{cred,backup_eligible}') = 'boolean'
            THEN
                (
                    wanted_new #- '{cred,counter}' #- '{cred,backup_state}'
                    #- '{cred,backup_eligible}'
                )::text
                = (
                    wanted_old #- '{cred,counter}' #- '{cred,backup_state}'
                    #- '{cred,backup_eligible}'
                )::text
                AND (wanted_new #>> '{cred,counter}')::bigint
                BETWEEN (wanted_old #>> '{cred,counter}')::bigint
                AND 4294967295
                AND (
                    (wanted_new #> '{cred,backup_eligible}')::boolean
                    OR NOT (wanted_old #> '{cred,backup_eligible}')::boolean
                )
        ELSE false
    END, false) THEN
        RAISE EXCEPTION 'a login changed more of a passkey than it may'
        USING ERRCODE = 'TD003';
    END IF;
    SELECT o.id, o.disabled_at INTO who, was
    FROM identity.passkeys AS p
    INNER JOIN identity.admins AS o ON p.admin_id = o.id
    WHERE p.credential_id = wanted_credential
    FOR SHARE OF o;
    IF NOT FOUND OR who IS DISTINCT FROM wanted_admin THEN
        RETURN NEXT;
        RETURN;
    ELSIF was IS NOT NULL THEN
        outcome := 'disabled';
        RETURN NEXT;
        RETURN;
    END IF;
    UPDATE identity.passkeys AS p
    SET passkey = wanted_new, last_used_at = clock_timestamp()
    WHERE
        p.credential_id = wanted_credential
        AND p.admin_id = wanted_admin
        AND p.passkey::text = wanted_old::text;
    IF NOT FOUND THEN
        -- A passkey removed while this waited is unknown; one changed is raced.
        outcome := CASE
            WHEN EXISTS (
                SELECT FROM identity.passkeys AS p
                WHERE p.credential_id = wanted_credential
            ) THEN 'raced'
        END;
        RETURN NEXT;
        RETURN;
    END IF;
    -- The session the browser presented ends, whoever's it was, so no token a
    -- browser held before logging in outlives it (ADR-0059, v5.0.0-7.2.4).
    DELETE FROM identity.sessions AS s
    WHERE s.token_hash = wanted_presented AND s.realm = 'admin';
    PERFORM identity.start_session(wanted_session, 'admin', who, true);
    outcome := 'logged_in';
    SELECT s.id::text INTO session_id
    FROM identity.sessions AS s
    WHERE s.token_hash = wanted_session;
    RETURN NEXT;
END
$$;

-- The account a live admin session names, if the admin is enabled: their
-- user handle and name, and a row for each passkey they hold, with how the
-- browser said it can reach it and whether it is synced, as the admin's
-- page and the browser's list of their passkeys need. An admin who holds
-- none has one row with no passkey. Reading it changes nothing, not even the
-- session's last request.
CREATE FUNCTION account(wanted_session bytea)
RETURNS TABLE (
    admin_id bigint,
    user_handle bytea,
    name text,
    credential_id bytea,
    transports text,
    backup_eligible boolean,
    backup_state boolean
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT
        o.id,
        uuid_send(o.user_handle),
        o.name,
        p.credential_id,
        p.transports,
        (p.passkey #>> '{cred,backup_eligible}')::boolean,
        (p.passkey #>> '{cred,backup_state}')::boolean
    FROM identity.sessions AS s
    INNER JOIN identity.admins AS o ON s.admin_id = o.id
    LEFT JOIN identity.passkeys AS p ON o.id = p.admin_id
    WHERE
        s.token_hash = wanted_session
        AND s.realm = 'admin'
        AND statement_timestamp() < s.ends_at
        AND statement_timestamp() < s.last_seen_at + interval '15 minutes'
        AND o.disabled_at IS NULL
    ORDER BY p.credential_id
$$;

-- The account a session names, as account finds it, if a passkey proved the
-- person in it within ten minutes, which makes it a fresh login (ADR-0048,
-- ADR-0069, v5.0.0-7.5.1),
-- and the admin holds fewer than two passkeys (ADR-0057): what adding a
-- passkey needs, with the passkeys they hold, so a device that has one makes
-- no second. Reading it changes nothing.
CREATE FUNCTION adder(wanted_session bytea)
RETURNS TABLE (
    admin_id bigint,
    user_handle bytea,
    name text,
    credential_id bytea,
    transports text,
    backup_eligible boolean,
    backup_state boolean
)
LANGUAGE sql STABLE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT a.*
    FROM identity.account(wanted_session) AS a
    INNER JOIN identity.sessions AS s ON s.token_hash = wanted_session
    WHERE
        s.proven_at > statement_timestamp() - interval '10 minutes'
        AND (
            SELECT count(*) FROM identity.passkeys AS p
            WHERE p.admin_id = a.admin_id
        ) < 2
$$;

-- Keeps a ceremony for adding a passkey under its token's hash, for ten
-- minutes, and says whether it did: only while the session is live and fresh,
-- names the admin, who is enabled and holds fewer than two passkeys,
-- checked here rather than trusted from the caller. It takes the admin's
-- row first, so a removal or a disable waits for it or is seen by it, under
-- read committed only (TD002). An admin has one such ceremony at a time, by
-- a unique index: a newer one replaces it. It remembers its session, which
-- alone may finish it.
CREATE FUNCTION begin_addition(
    wanted_hash bytea,
    wanted_session bytea,
    wanted_admin bigint,
    wanted_state jsonb
)
RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    -- The guard, the admin's row, and the sweep come first.
    at timestamptz := identity.holding(wanted_admin);
BEGIN
    IF at IS NULL THEN
        RETURN false;
    END IF;
    BEGIN
        INSERT INTO identity.ceremonies
            (token_hash, kind, admin_id, session_hash, state, ends_at)
        SELECT
            wanted_hash,
            'add',
            s.admin_id,
            s.token_hash,
            wanted_state,
            at + interval '10 minutes'
        FROM identity.sessions AS s
        WHERE
            s.token_hash = wanted_session
            AND s.admin_id = wanted_admin
            AND s.realm = 'admin'
            AND at < s.ends_at
            AND at < s.last_seen_at + interval '15 minutes'
            AND s.proven_at > at - interval '10 minutes'
            AND (
                SELECT count(*) FROM identity.passkeys AS p
                WHERE p.admin_id = wanted_admin
            ) < 2
        ON CONFLICT (admin_id) WHERE kind = 'add' DO UPDATE
            SET
                token_hash = excluded.token_hash,
                session_hash = excluded.session_hash,
                taken = false,
                state = excluded.state,
                ends_at = excluded.ends_at;
    EXCEPTION WHEN foreign_key_violation THEN
        -- The session ended while this waited for its row.
        RETURN false;
    END;
    RETURN FOUND;
END
$$;

-- Takes an open addition by its token's hash, for the session that began it,
-- and returns its admin and state. It is marked taken, so it is taken once,
-- and only storing its passkey uses it up; a session that did not begin it
-- leaves it as it was.
CREATE FUNCTION take_addition(wanted_hash bytea, wanted_session bytea)
RETURNS TABLE (admin_id bigint, state text)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    UPDATE identity.ceremonies AS c
    SET taken = true
    WHERE
        c.token_hash = wanted_hash
        AND c.kind = 'add'
        AND c.session_hash = wanted_session
        AND NOT c.taken
        AND c.ends_at > clock_timestamp()
    RETURNING c.admin_id, c.state::text
$$;

-- Stores an admin's added passkey and ends their other sessions, keeping the
-- one that added it (ADR-0059, v5.0.0-7.4.3), and says what came of it, with
-- how many live sessions it ended and the adding session's public ID: added;
-- disabled; logged_out, for a session that ended or is not theirs; ended, for
-- no open addition this session took, which leaves any other as it is; the one
-- it finds it uses up, so the database itself ties the passkey to an addition a
-- fresh session began; full, for an admin who holds two already (ADR-0057);
-- oversized, as for a first passkey; taken, for a credential another passkey
-- has, even one stored a moment before; or null for an admin who does not
-- exist. It takes the admin's row for update, so what it checks stays true
-- until it commits, under read committed only (TD002). The session need not
-- still be fresh: an addition begun from a fresh one may take its ten minutes.
CREATE FUNCTION add_passkey(
    wanted_addition bytea,
    wanted_session bytea,
    wanted_admin bigint,
    wanted_credential bytea,
    wanted_passkey jsonb,
    wanted_transports text
)
RETURNS TABLE (added text, sessions_ended bigint, session_id text)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    was timestamptz;
BEGIN
    PERFORM identity.require_read_committed('adding a passkey');
    SELECT o.disabled_at INTO was
    FROM identity.admins AS o
    WHERE o.id = wanted_admin
    FOR NO KEY UPDATE;
    at := clock_timestamp();
    sessions_ended := 0;
    IF NOT FOUND THEN
        RETURN NEXT;
        RETURN;
    ELSIF was IS NOT NULL THEN
        added := 'disabled';
        RETURN NEXT;
        RETURN;
    END IF;
    SELECT s.id::text INTO session_id
    FROM identity.sessions AS s
    WHERE
        s.token_hash = wanted_session
        AND s.admin_id = wanted_admin
        AND s.realm = 'admin'
        AND at < s.ends_at
        AND at < s.last_seen_at + interval '15 minutes';
    IF NOT FOUND THEN
        added := 'logged_out';
        RETURN NEXT;
        RETURN;
    END IF;
    DELETE FROM identity.ceremonies AS c
    WHERE
        c.token_hash = wanted_addition
        AND c.kind = 'add'
        AND c.taken
        AND c.session_hash = wanted_session
        AND c.admin_id = wanted_admin
        AND c.ends_at > at;
    IF NOT FOUND THEN
        added := 'ended';
    ELSIF (
        SELECT count(*) FROM identity.passkeys AS p
        WHERE p.admin_id = wanted_admin
    ) >= 2 THEN
        added := 'full';
    ELSIF
        octet_length(wanted_credential) NOT BETWEEN 16 AND 1023
        OR octet_length(wanted_passkey::text) > 8192
    THEN
        added := 'oversized';
    ELSE
        BEGIN
            INSERT INTO identity.passkeys
                (credential_id, admin_id, passkey, transports, created_at)
            VALUES (
                wanted_credential,
                wanted_admin,
                wanted_passkey,
                wanted_transports,
                at
            );
            added := 'added';
            -- Every other session ends; the one that added stays
            -- (ADR-0059, v5.0.0-7.4.3).
            WITH gone AS (
                DELETE FROM identity.sessions AS s
                WHERE
                    s.admin_id = wanted_admin
                    AND s.token_hash <> wanted_session
                RETURNING
                    at < s.ends_at
                    AND at < s.last_seen_at + CASE s.realm
                        WHEN 'admin' THEN interval '15 minutes'
                    END AS live
            )

            SELECT count(*) FILTER (WHERE gone.live) INTO sessions_ended
            FROM gone;
        EXCEPTION WHEN unique_violation THEN
            added := 'taken';
        END;
    END IF;
    RETURN NEXT;
END
$$;

-- Takes a join or login ceremony by its token's hash, once: it is
-- removed whether or not it is still open, and returned only if it is.
-- Additions are taken by take_addition, for their session alone.
CREATE FUNCTION take_ceremony(wanted_hash bytea, wanted_kind text)
RETURNS TABLE (admin_id bigint, link_hash bytea, state text)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH taken AS (
        DELETE FROM identity.ceremonies AS c
        WHERE
            c.token_hash = wanted_hash
            AND c.kind = wanted_kind
            AND c.kind <> 'add'
        RETURNING c.admin_id, c.link_hash, c.state, c.ends_at
    )

    SELECT
        taken.admin_id,
        taken.link_hash,
        taken.state::text
    FROM taken
    WHERE taken.ends_at > clock_timestamp()
$$;

-- Stores an admin's first passkey and uses up the link that let them, in
-- one transaction, and says what came of it: redeemed; disabled; joined,
-- for an admin who has a passkey already, as when two tabs finish one
-- link; oversized, for a credential ID outside the 16 to 1023 bytes WebAuthn
-- allows or a passkey larger than the table keeps, measured as the table
-- measures it; expired, for a link that ended or was replaced; taken, for a
-- credential another admin's passkey has, even one stored a moment before
-- (the admin's own passkeys were refused as joined already); or null
-- for an admin who does not exist. It takes the admin's row first, so a
-- link being issued, a disable, or another tab's passkey waits, and what it
-- checks stays true until it commits, under read committed only, so it
-- refuses any other level with TD002. Redeemed, it ends the session the
-- browser presented and starts the admin's, unproven: it rests on the link,
-- so it is never fresh until a passkey login proves them (ADR-0069).
CREATE FUNCTION redeem_invite(
    wanted_link bytea,
    wanted_admin bigint,
    wanted_credential bytea,
    wanted_passkey jsonb,
    wanted_transports text,
    wanted_session bytea,
    wanted_presented bytea
)
RETURNS text
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    standing text;
BEGIN
    standing := identity.joining(wanted_admin);
    IF standing IS DISTINCT FROM 'open' THEN
        RETURN standing;
    ELSIF
        octet_length(wanted_credential) NOT BETWEEN 16 AND 1023
        OR octet_length(wanted_passkey::text) > 8192
    THEN
        RETURN 'oversized';
    END IF;
    -- A credential another admin's passkey has, however recently, undoes
    -- the link's use and is a refusal, recorded like any other.
    BEGIN
        DELETE FROM identity.invites AS e
        WHERE
            e.token_hash = wanted_link
            AND e.admin_id = wanted_admin
            AND e.ends_at > clock_timestamp();
        IF NOT FOUND THEN
            RETURN 'expired';
        END IF;
        INSERT INTO identity.passkeys
            (credential_id, admin_id, passkey, transports, created_at)
        VALUES (
            wanted_credential,
            wanted_admin,
            wanted_passkey,
            wanted_transports,
            clock_timestamp()
        );
    EXCEPTION WHEN unique_violation THEN
        RETURN 'taken';
    END;
    -- Any other ceremony of theirs could only be refused now; it goes too.
    DELETE FROM identity.ceremonies AS c WHERE c.admin_id = wanted_admin;
    DELETE FROM identity.sessions AS s
    WHERE s.token_hash = wanted_presented AND s.realm = 'admin';
    PERFORM identity.start_session(
        wanted_session, 'admin', wanted_admin, false
    );
    RETURN 'redeemed';
END
$$;

-- Ends every session an admin holds, in any realm, and says how many were
-- live as of `at`, spent ones going too. Only disabling and removing passkeys
-- call it, holding the admin's row; no other role may.
CREATE FUNCTION end_every_session(wanted_admin bigint, at timestamptz)
RETURNS bigint
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH gone AS (
        DELETE FROM identity.sessions AS s
        WHERE s.admin_id = wanted_admin
        RETURNING
            at < s.ends_at
            AND at < s.last_seen_at + CASE s.realm
                WHEN 'admin' THEN interval '15 minutes'
            END AS live
    )

    SELECT count(*) FILTER (WHERE gone.live) FROM gone
$$;

-- Disables an admin, then ends every session it holds in any realm, its
-- invite link, and any passkey ceremony under way (v5.0.0-7.4.2), and
-- says whether it acted and how many live sessions and links it ended, spent
-- ones going too: done is false for an admin already
-- disabled, and null for one that does not exist. Each step is its own
-- statement, so the deletes see what a login or link committed while the
-- lock waited on the admin's row. One statement would not, and nor would
-- repeatable read, which it refuses with TD002, as do enable, end_sessions,
-- and the join functions.
CREATE FUNCTION disable_admin(wanted_admin bigint)
RETURNS TABLE (done boolean, sessions_ended bigint, links_ended bigint)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    was timestamptz;
BEGIN
    PERFORM identity.require_read_committed('disabling');
    sessions_ended := 0;
    links_ended := 0;
    -- The row first, waiting out any enable, so its answer is the state it
    -- leaves, never "already disabled" over an enable about to commit.
    SELECT o.disabled_at INTO was
    FROM identity.admins AS o
    WHERE o.id = wanted_admin
    FOR NO KEY UPDATE;
    IF NOT FOUND OR was IS NOT NULL THEN
        done := CASE WHEN FOUND THEN false END;
        RETURN NEXT;
        RETURN;
    END IF;
    at := clock_timestamp();
    UPDATE identity.admins AS o SET disabled_at = at
    WHERE o.id = wanted_admin;
    sessions_ended := identity.end_every_session(wanted_admin, at);
    WITH gone AS (
        DELETE FROM identity.invites AS e
        WHERE e.admin_id = wanted_admin
        RETURNING at < e.ends_at AS live
    )

    SELECT count(*) FILTER (WHERE gone.live) INTO links_ended FROM gone;
    DELETE FROM identity.ceremonies AS c WHERE c.admin_id = wanted_admin;
    done := true;
    RETURN NEXT;
END
$$;

-- Removes every passkey an admin holds, so they can be issued a link and
-- join again, as when their only passkey is lost or unusable (ADR-0057).
-- It ends their sessions and any ceremony too, since a session begun with a
-- removed passkey must not outlive it (v5.0.0-7.4.3). Says how many passkeys
-- it removed and how many live sessions it ended, or nothing for an admin
-- who does not exist. Like a disable, it takes the admin's row for update
-- first, so a login or a join being stored waits for it or is seen by
-- it, under read committed only, so it refuses any other level with TD002.
CREATE FUNCTION remove_passkeys(wanted_admin bigint)
RETURNS TABLE (passkeys_removed bigint, sessions_ended bigint)
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
BEGIN
    PERFORM identity.require_read_committed('removing passkeys');
    PERFORM
    FROM identity.admins AS o
    WHERE o.id = wanted_admin
    FOR NO KEY UPDATE;
    IF NOT FOUND THEN
        RETURN;
    END IF;
    at := clock_timestamp();
    WITH gone AS (
        DELETE FROM identity.passkeys AS p
        WHERE p.admin_id = wanted_admin
        RETURNING p.admin_id
    )

    SELECT count(*) INTO passkeys_removed FROM gone;
    sessions_ended := identity.end_every_session(wanted_admin, at);
    DELETE FROM identity.ceremonies AS c WHERE c.admin_id = wanted_admin;
    RETURN NEXT;
END
$$;

-- Enables a disabled admin again, and says whether it acted: false for an
-- admin already enabled, null for one that does not exist. Like a disable,
-- it takes the row first, so its answer is the state it leaves, which holds
-- only under read committed, so it refuses any other level with TD002. Their
-- sessions stay ended, and they log in afresh.
CREATE FUNCTION enable_admin(wanted_admin bigint)
RETURNS boolean
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    was timestamptz;
BEGIN
    PERFORM identity.require_read_committed('enabling');
    SELECT o.disabled_at INTO was
    FROM identity.admins AS o
    WHERE o.id = wanted_admin
    FOR NO KEY UPDATE;
    IF NOT FOUND THEN
        RETURN null;
    END IF;
    UPDATE identity.admins AS o SET disabled_at = null
    WHERE o.id = wanted_admin;
    RETURN was IS NOT NULL;
END
$$;

-- Ends every session one admin holds in a realm, or every admin's when
-- none is named, and says how many were live, spent ones going too, or null
-- for an admin who does not exist (v5.0.0-7.4.5). It first takes the
-- admins' rows, so a login that holds one finishes before the delete,
-- which then ends it too. Admins are few, so taking every row holds little.
CREATE FUNCTION end_sessions(wanted_realm text, wanted_admin bigint)
RETURNS bigint
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    ended bigint;
BEGIN
    PERFORM identity.require_read_committed('ending sessions');
    PERFORM
    FROM identity.admins AS o
    WHERE wanted_admin IS NULL OR o.id = wanted_admin
    ORDER BY o.id
    FOR NO KEY UPDATE;
    IF NOT FOUND AND wanted_admin IS NOT NULL THEN
        RETURN null;
    END IF;
    at := clock_timestamp();
    WITH gone AS (
        DELETE FROM identity.sessions AS s
        WHERE
            s.realm = wanted_realm
            AND (wanted_admin IS NULL OR s.admin_id = wanted_admin)
        RETURNING
            at < s.ends_at
            AND at < s.last_seen_at + CASE s.realm
                WHEN 'admin' THEN interval '15 minutes'
            END AS live
    )

    SELECT count(*) FILTER (WHERE gone.live) INTO ended FROM gone;
    RETURN ended;
END
$$;

-- No routine here is anyone's by default; Identity's role may run exactly the
-- lookups it calls, so a helper added later is no one's until granted. It may
-- not call start_session; only login and redeem_invite do. The database cannot
-- check a passkey's signature, though, so a caller with this role can do what
-- an admin can, for any admin: start a session through login and add a
-- passkey through an addition that session took, or remove an admin's
-- passkeys and join, and log in, with its own through a new link. Database
-- access stays the trust boundary (ADR-0061).
REVOKE EXECUTE ON ALL ROUTINES IN SCHEMA identity FROM public;
GRANT EXECUTE ON FUNCTION use_session(bytea, text), end_session(bytea, text),
issue_invite(bytea, bigint), list_admins(), invitee(bytea),
begin_join(bytea, bigint, bytea, jsonb), take_ceremony(bytea, text),
begin_login(bytea, jsonb), signer(bytea), account(bytea),
login(bytea, bigint, jsonb, jsonb, bytea, bytea), adder(bytea),
begin_addition(bytea, bytea, bigint, jsonb), take_addition(bytea, bytea),
add_passkey(bytea, bytea, bigint, bytea, jsonb, text),
redeem_invite(bytea, bigint, bytea, jsonb, text, bytea, bytea),
disable_admin(bigint), enable_admin(bigint), remove_passkeys(bigint),
end_sessions(text, bigint)
TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION start_session(bytea, text, bigint, boolean)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION use_session(bytea, text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_session(bytea, text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION require_read_committed(text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION joining(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION holding(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_every_session(bigint, timestamptz)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION issue_invite(bytea, bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION list_admins() OWNER TO tetsu_identity_lookup;
ALTER FUNCTION invitee(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION begin_join(bytea, bigint, bytea, jsonb)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION take_ceremony(bytea, text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION begin_login(bytea, jsonb) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION account(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION adder(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION begin_addition(bytea, bytea, bigint, jsonb)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION take_addition(bytea, bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION add_passkey(bytea, bytea, bigint, bytea, jsonb, text)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION signer(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION login(bytea, bigint, jsonb, jsonb, bytea, bytea)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION redeem_invite(bytea, bigint, bytea, jsonb, text, bytea, bytea)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION disable_admin(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION enable_admin(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION remove_passkeys(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_sessions(text, bigint) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
