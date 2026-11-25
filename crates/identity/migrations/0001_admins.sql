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
CREATE TABLE admin_sessions (
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
    ends_at timestamptz NOT NULL,
    -- The device a session started on, as coarsely as the list of sessions
    -- shows it, such as Firefox on Linux, and none when the browser named none
    -- (ADR-0059).
    device text CHECK (device IS NULL OR length(device) BETWEEN 1 AND 64)
)
-- Every logged-in request rewrites last_seen_at, which no index names, so
-- room on the page keeps each rewrite there (ADR-0060).
WITH (fillfactor = 80);

COMMENT ON TABLE admin_sessions IS
'tetsu:global: found by its token before any business is known';

CREATE INDEX admin_sessions_admin_id ON admin_sessions (admin_id);

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
    REFERENCES admin_sessions (token_hash) ON DELETE CASCADE,
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
REVOKE ALL ON admins, admin_sessions, invites, passkeys, ceremonies
FROM tetsu_identity;
REVOKE ALL ON SEQUENCE admins_id_seq FROM tetsu_identity;

-- Identity's role adds and reads admins, and reaches sessions and links
-- only through functions, one token's hash a call, so no caller can list
-- them. Those functions read whether an admin is disabled, and lock its row
-- while they do, so a disable at the same moment never misses what they add.
-- What the login flow's functions need is granted with them, in 0014; what
-- only admins' own functions need is granted here.
GRANT SELECT (id, name), INSERT (name) ON admins TO tetsu_identity;
-- Replacing a link reads every column its upsert sets, issued_at included.
GRANT SELECT, INSERT, DELETE ON invites TO tetsu_identity_lookup;
GRANT UPDATE (token_hash, issued_at, ends_at) ON invites
TO tetsu_identity_lookup;
GRANT SELECT (created_at) ON admins TO tetsu_identity_lookup;
GRANT DELETE ON passkeys TO tetsu_identity_lookup;

-- Each realm's limits live here, beside the clock that applies them, so no
-- caller can lengthen a session by passing other numbers (ADR-0048). Each
-- function reads the clock once, so neither a server's clock nor a request
-- handled late can stretch a session. A use that waits for another's lock on
-- the same row is judged as of its own start, a wait no longer than the lock
-- timeout; a start reads the clock after its wait, so it is never dated
-- before what held it up.

-- How long a realm's sessions last (ADR-0048): at most absolute_limit after
-- they start, and idle_limit after their last request. The one place each is
-- written in SQL; Realm::limits says the same in Rust, and tests check that
-- the two agree (#142). A realm with none has no live session.
CREATE FUNCTION absolute_limit(wanted_realm text)
RETURNS interval
LANGUAGE sql IMMUTABLE
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT CASE wanted_realm
        WHEN 'admin' THEN interval '8 hours'
        WHEN 'staff' THEN interval '12 hours'
    END
$$;

CREATE FUNCTION idle_limit(wanted_realm text)
RETURNS interval
LANGUAGE sql IMMUTABLE
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT CASE wanted_realm
        WHEN 'admin' THEN interval '15 minutes'
        WHEN 'staff' THEN interval '1 hour'
    END
$$;

-- How many passkeys a realm's account may hold: two for an admin (ADR-0057),
-- ten for a staff member, and none for a realm it does not know (ADR-0091).
CREATE FUNCTION passkey_cap(wanted_realm text)
RETURNS bigint
LANGUAGE sql IMMUTABLE
SET search_path = pg_catalog, pg_temp
AS $$
    SELECT CASE wanted_realm
        WHEN 'admin' THEN 2
        WHEN 'staff' THEN 10
        ELSE 0
    END
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
    DELETE FROM identity.admin_sessions AS s
    WHERE s.token_hash = wanted_presented AND s.realm = 'admin';
    PERFORM identity.start_session(
        wanted_session, 'admin', wanted_admin, false
    );
    RETURN 'redeemed';
END
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
        DELETE FROM identity.admin_sessions AS s
        WHERE
            s.realm = wanted_realm
            AND (wanted_admin IS NULL OR s.admin_id = wanted_admin)
        RETURNING
            at < s.ends_at
            AND at < s.last_seen_at + identity.idle_limit(s.realm) AS live
    )

    SELECT count(*) FILTER (WHERE gone.live) INTO ended FROM gone;
    RETURN ended;
END
$$;

-- No routine here is anyone's by default; Identity's role may run exactly the
-- lookups it calls, so a helper added later is no one's until granted. The
-- database cannot check a passkey's signature, though, so a caller with this
-- role can do what an admin can, for any admin: remove an admin's passkeys,
-- and join, and log in, with its own through a new link. Database access stays
-- the trust boundary (ADR-0061).
REVOKE EXECUTE ON ALL ROUTINES IN SCHEMA identity FROM public;
-- The limits run as whoever calls them, which is a lookup function's owner.
GRANT EXECUTE ON FUNCTION absolute_limit(text), idle_limit(text),
passkey_cap(text)
TO tetsu_identity_lookup;
GRANT EXECUTE ON FUNCTION issue_invite(bytea, bigint), list_admins(),
invitee(bytea), begin_join(bytea, bigint, bytea, jsonb),
redeem_invite(bytea, bigint, bytea, jsonb, text, bytea, bytea),
disable_admin(bigint), enable_admin(bigint), remove_passkeys(bigint),
end_sessions(text, bigint)
TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION require_read_committed(text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION joining(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION issue_invite(bytea, bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION list_admins() OWNER TO tetsu_identity_lookup;
ALTER FUNCTION invitee(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION begin_join(bytea, bigint, bytea, jsonb)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION redeem_invite(bytea, bigint, bytea, jsonb, text, bytea, bytea)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION disable_admin(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION enable_admin(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION remove_passkeys(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION end_sessions(text, bigint) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
