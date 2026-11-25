-- The login flow every realm shares, written once as the admin realm's
-- (ADR-0091): starting, using, and ending sessions, logging in, and adding
-- passkeys, and joining by an invite link up to using the link up. Version 15
-- renders this file for staff, swapping each name of the admin realm's for
-- staff's own, so each realm's functions read only its own tables. Every name a
-- swap changes is written whole and schema-qualified, and a test fails if the
-- staff copy still names anything of the admins'. Using a link up, accepting
-- an invite, and managing accounts differ by realm and are written out apart:
-- admins' in 0001, whose join functions call these, and staff's in 0016.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- Only through the functions below, which run as their owner. The public ID
-- too, which audit records name a session by, never its hash.
GRANT SELECT (
    token_hash, id, realm, admin_id, started_at, proven_at, last_seen_at,
    ends_at, device
),
INSERT (
    token_hash, realm, admin_id, started_at, proven_at, last_seen_at,
    ends_at
),
UPDATE (last_seen_at, device),
DELETE ON identity.admin_sessions TO tetsu_identity_lookup;
-- Locking an account's row needs a right to update it.
GRANT SELECT (id, name, email, user_handle, disabled_at), UPDATE (disabled_at)
ON identity.admins TO tetsu_identity_lookup;
GRANT SELECT (credential_id, admin_id, passkey, transports),
INSERT (credential_id, admin_id, passkey, transports, created_at),
UPDATE (passkey, last_used_at)
ON identity.passkeys TO tetsu_identity_lookup;
GRANT SELECT, DELETE ON identity.ceremonies TO tetsu_identity_lookup;
-- An invite is read here and used up where each realm keeps its links. The
-- admins' lookup reads every column already (0001); this grant is for the
-- staff copy, which reads only these.
GRANT SELECT (token_hash, admin_id, ends_at) ON identity.invites
TO tetsu_identity_lookup;
GRANT INSERT (
    token_hash, kind, admin_id, link_hash, session_hash, state, ends_at
) ON identity.ceremonies TO tetsu_identity_lookup;
GRANT UPDATE (token_hash, session_hash, taken, state, ends_at)
ON identity.ceremonies TO tetsu_identity_lookup;

-- Starts a session now, only for an enabled admin, and removes the
-- admin's spent sessions in the realm, so they do not pile up. Only a login
-- and a redemption call it, each holding an enabled admin's row, so one
-- disabled or unknown is a fault, raised as TD001, Identity's own code. A
-- session is proven only when a passkey login starts it (ADR-0069). It holds
-- the admin's row until it commits, so a disable waits for it and then ends
-- the session it started.
CREATE FUNCTION identity.start_session(
    wanted_hash bytea, wanted_realm text, wanted_admin bigint,
    wanted_proven boolean
)
RETURNS void
LANGUAGE plpgsql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
DECLARE
    at timestamptz;
    absolute interval := identity.absolute_limit(wanted_realm);
    idle interval := identity.idle_limit(wanted_realm);
BEGIN
    PERFORM
    FROM identity.admins AS o
    WHERE o.id = wanted_admin AND o.disabled_at IS NULL
    FOR SHARE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'no enabled account' USING ERRCODE = 'TD001';
    END IF;
    -- After any wait for the admin's row, so the session starts when it
    -- was stored, never before an end of sessions that held it up.
    at := clock_timestamp();
    DELETE FROM identity.admin_sessions AS s
    WHERE
        s.admin_id = wanted_admin
        AND s.realm = wanted_realm
        AND (at >= s.ends_at OR at >= s.last_seen_at + idle);
    INSERT INTO identity.admin_sessions (
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
CREATE FUNCTION identity.use_session(wanted_hash bytea, wanted_realm text)
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
    idle interval := identity.idle_limit(wanted_realm);
BEGIN
    RETURN QUERY
    UPDATE identity.admin_sessions AS s
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
        DELETE FROM identity.admin_sessions AS s
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

-- Whole seconds to each limit on the live session a token's hash names in a
-- realm, by the database's clock, as use_session reads it: none for a session
-- spent, of another realm, or of a disabled admin. It changes and removes
-- nothing; use_session removes a spent session when it is next presented. Its
-- idle limit must match use_session's, which a test checks; #142 gives the
-- limits one home.
CREATE FUNCTION identity.session_left(wanted_hash bytea, wanted_realm text)
RETURNS TABLE (idle_left bigint, absolute_left bigint)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH clock AS (
        SELECT
            clock_timestamp() AS at,
            identity.idle_limit(wanted_realm)
                AS idle
    )

    SELECT
        floor(
            extract(EPOCH FROM s.last_seen_at + clock.idle - clock.at)
        )::bigint,
        floor(extract(EPOCH FROM s.ends_at - clock.at))::bigint
    FROM identity.admin_sessions AS s
    INNER JOIN identity.admins AS o ON s.admin_id = o.id
    CROSS JOIN clock
    WHERE
        s.token_hash = wanted_hash
        AND s.realm = wanted_realm
        AND clock.at < s.ends_at
        AND clock.at < s.last_seen_at + clock.idle
        AND o.disabled_at IS NULL
$$;

-- Names the device a session just started on, once: login and joining call it
-- with the session they start, in the same statement. A session already named
-- is left as it is.
CREATE FUNCTION identity.describe_session(wanted_hash bytea, wanted_device text)
RETURNS void
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    UPDATE identity.admin_sessions AS s
    SET device = wanted_device
    WHERE s.token_hash = wanted_hash AND s.device IS NULL
$$;

-- Removes a session in a realm, and says whether it was live when it ended,
-- with its public id and account: a spent one is removed too, but it had
-- already ended. No row means there was none.
CREATE FUNCTION identity.log_out(wanted_hash bytea, wanted_realm text)
RETURNS TABLE (live boolean, session_id uuid, account_id bigint)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH clock AS (
        SELECT
            clock_timestamp() AS at,
            identity.idle_limit(wanted_realm)
                AS idle
    )

    DELETE FROM identity.admin_sessions AS s
    USING clock
    WHERE s.token_hash = wanted_hash AND s.realm = wanted_realm
    RETURNING
        coalesce(clock.at < s.ends_at AND clock.at < s.last_seen_at + clock.idle,
            false),
        s.id,
        s.admin_id
$$;

-- Takes an enabled admin's row in share mode, as a ceremony that will add
-- a passkey for them begins, so a disable or a passkey being stored waits for
-- it or is seen by it, sweeps the admin's ceremonies that have ended, and
-- says when, by the clock read after any wait; null for an admin not
-- enabled. Under read committed only (TD002). Only the
-- functions that begin such ceremonies call it; no other role may.
CREATE FUNCTION identity.holding(wanted_admin bigint)
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

-- Where an admin stands for join, after taking their row for update,
-- since both callers change what the admin holds: open, disabled, joined
-- for one who holds a passkey already, or null for one who does not exist.
-- What it checks stays true until the caller commits, under read committed
-- only, so it refuses any other level with TD002. Only Identity's functions
-- that issue or redeem a link call it; no other role may.
CREATE FUNCTION identity.joining(wanted_admin bigint)
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
CREATE FUNCTION identity.begin_join(
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

-- Joins an account by its invite link: stores its first passkey and starts its
-- session, and says what came of it: redeemed; disabled; joined, for an
-- account that has a passkey already, as when two tabs finish one link;
-- oversized, for a credential ID outside the 16 to 1023 bytes WebAuthn allows
-- or a passkey larger than the table keeps, measured as the table measures
-- it; expired, for a link that ended, was replaced, or names someone else;
-- taken, for a credential another account's passkey has, even one stored a
-- moment before; or null for an account that does not exist. It reads the
-- link and leaves it for its caller to use up in the same transaction, which
-- each realm does where its links live. It takes the account's row first, so
-- a link being issued, a disable, or another tab's passkey waits, and what it
-- checks stays true until it commits, under read committed only (TD002). The
-- session it starts rests on the link, so it is never fresh until a passkey
-- login proves them (ADR-0069). Only a redemption calls it; no other role may.
CREATE FUNCTION identity.join_by_link(
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
    ELSIF NOT EXISTS (
        SELECT FROM identity.invites AS e
        WHERE
            e.token_hash = wanted_link
            AND e.admin_id = wanted_admin
            AND e.ends_at > clock_timestamp()
    ) THEN
        RETURN 'expired';
    END IF;
    BEGIN
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

-- Keeps a login ceremony's state for ten minutes under its token's hash.
-- Anyone may begin one, so until step 5's rate limits two bounds keep the
-- table small and never refuse a begin (ADR-0048, v5.0.0-6.3.1): each begin
-- sweeps up to a hundred ended ceremonies, and past ten thousand open logins
-- it ends the oldest. A flood of begins can still end a real ceremony before
-- its person answers, until those rate limits. Both skip rows another
-- transaction holds, so begins never wait on each other, under read committed
-- only (TD002), where a skipped row is never a serialisation failure.
CREATE FUNCTION identity.begin_login(wanted_hash bytea, wanted_state jsonb)
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

-- Takes a join or login ceremony by its token's hash, once: it is
-- removed whether or not it is still open, and returned only if it is.
-- Additions are taken by take_addition, for their session alone.
CREATE FUNCTION identity.take_ceremony(wanted_hash bytea, wanted_kind text)
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

-- The passkey a credential ID names, with its admin, their user handle,
-- and whether they are disabled, so a refused login names whose it was;
-- nothing for a credential no admin holds. Reading it changes nothing.
CREATE FUNCTION identity.signer(wanted_credential bytea)
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
CREATE FUNCTION identity.login(
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
    DELETE FROM identity.admin_sessions AS s
    WHERE s.token_hash = wanted_presented AND s.realm = 'admin';
    PERFORM identity.start_session(wanted_session, 'admin', who, true);
    outcome := 'logged_in';
    SELECT s.id::text INTO session_id
    FROM identity.admin_sessions AS s
    WHERE s.token_hash = wanted_session;
    RETURN NEXT;
END
$$;

-- The account a live admin session names, if the admin is enabled: their
-- user handle, name, and email, and a row for each passkey they hold, with
-- how the browser said it can reach it and whether it is synced, as the
-- admin's page and the browser's list of their passkeys need. An admin who
-- holds none has one row with no passkey. Reading it changes nothing, not even
-- the session's last request.
CREATE FUNCTION identity.account(wanted_session bytea)
RETURNS TABLE (
    admin_id bigint,
    user_handle bytea,
    name text,
    email text,
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
        o.email,
        p.credential_id,
        p.transports,
        (p.passkey #>> '{cred,backup_eligible}')::boolean,
        (p.passkey #>> '{cred,backup_state}')::boolean
    FROM identity.admin_sessions AS s
    INNER JOIN identity.admins AS o ON s.admin_id = o.id
    LEFT JOIN identity.passkeys AS p ON o.id = p.admin_id
    WHERE
        s.token_hash = wanted_session
        AND s.realm = 'admin'
        AND statement_timestamp() < s.ends_at
        AND statement_timestamp() < s.last_seen_at + identity.idle_limit(s.realm)
        AND o.disabled_at IS NULL
    ORDER BY p.credential_id
$$;

-- The live admin sessions of whoever's live session a token's hash names,
-- the presented one first and then the most recently used: each by its random
-- id, never its token's hash, with when it started and was last used, and its
-- device. The admin is found from the presented session alone, so no request
-- lists another admin's sessions. It renews nothing. Its idle limit must
-- match use_session's, as account's does; #142 gives the limits one home.
CREATE FUNCTION identity.sessions_of(wanted_session bytea)
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
        AND statement_timestamp() < mine.last_seen_at + identity.idle_limit(mine.realm)
        AND o.disabled_at IS NULL
        AND statement_timestamp() < s.ends_at
        AND statement_timestamp() < s.last_seen_at + identity.idle_limit(s.realm)
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
CREATE FUNCTION identity.end_other_sessions(
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
        AND at < s.last_seen_at + identity.idle_limit(s.realm);
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
            at < s.ends_at AND at < s.last_seen_at + identity.idle_limit(s.realm)
                AS live
    )

    SELECT count(*) FILTER (WHERE gone.live) INTO ended FROM gone;
END
$$;

-- The account a session names, as account finds it, if a passkey proved the
-- person in it within ten minutes, which makes it a fresh login (ADR-0048,
-- ADR-0069, v5.0.0-7.5.1),
-- and the admin holds fewer than two passkeys (ADR-0057): what adding a
-- passkey needs, with the passkeys they hold, so a device that has one makes
-- no second. Reading it changes nothing.
CREATE FUNCTION identity.adder(wanted_session bytea)
RETURNS TABLE (
    admin_id bigint,
    user_handle bytea,
    name text,
    email text,
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
    INNER JOIN identity.admin_sessions AS s ON s.token_hash = wanted_session
    WHERE
        s.proven_at > statement_timestamp() - interval '10 minutes'
        AND (
            SELECT count(*) FROM identity.passkeys AS p
            WHERE p.admin_id = a.admin_id
        ) < identity.passkey_cap('admin')
$$;

-- Keeps a ceremony for adding a passkey under its token's hash, for ten
-- minutes, and says whether it did: only while the session is live and fresh,
-- names the admin, who is enabled and holds fewer than two passkeys,
-- checked here rather than trusted from the caller. It takes the admin's
-- row first, so a removal or a disable waits for it or is seen by it, under
-- read committed only (TD002). An admin has one such ceremony at a time, by
-- a unique index: a newer one replaces it. It remembers its session, which
-- alone may finish it.
CREATE FUNCTION identity.begin_addition(
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
        FROM identity.admin_sessions AS s
        WHERE
            s.token_hash = wanted_session
            AND s.admin_id = wanted_admin
            AND s.realm = 'admin'
            AND at < s.ends_at
            AND at < s.last_seen_at + identity.idle_limit(s.realm)
            AND s.proven_at > at - interval '10 minutes'
            AND (
                SELECT count(*) FROM identity.passkeys AS p
                WHERE p.admin_id = wanted_admin
            ) < identity.passkey_cap('admin')
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
CREATE FUNCTION identity.take_addition(wanted_hash bytea, wanted_session bytea)
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
CREATE FUNCTION identity.add_passkey(
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
    FROM identity.admin_sessions AS s
    WHERE
        s.token_hash = wanted_session
        AND s.admin_id = wanted_admin
        AND s.realm = 'admin'
        AND at < s.ends_at
        AND at < s.last_seen_at + identity.idle_limit(s.realm);
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
    ) >= identity.passkey_cap('admin') THEN
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
                DELETE FROM identity.admin_sessions AS s
                WHERE
                    s.admin_id = wanted_admin
                    AND s.token_hash <> wanted_session
                RETURNING
                    at < s.ends_at
                    AND at < s.last_seen_at + identity.idle_limit(s.realm) AS live
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

-- Ends every session an admin holds, in any realm, and says how many were
-- live as of `at`, spent ones going too. Only disabling and removing passkeys
-- call it, holding the admin's row; no other role may.
CREATE FUNCTION identity.end_every_session(wanted_admin bigint, at timestamptz)
RETURNS bigint
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH gone AS (
        DELETE FROM identity.admin_sessions AS s
        WHERE s.admin_id = wanted_admin
        RETURNING
            at < s.ends_at
            AND at < s.last_seen_at + identity.idle_limit(s.realm) AS live
    )

    SELECT count(*) FILTER (WHERE gone.live) FROM gone
$$;

-- No routine is anyone's by default: the runner's own defaults keep PUBLIC
-- from running what it creates, and the isolation check fails on any function
-- PUBLIC may run (ADR-0042). No statement here revokes on the schema's other
-- functions, which a runner that is no superuser may not do once another role
-- owns them. Identity's role may run the ones it calls; start_session,
-- holding, joining, and end_every_session only other functions call, and
-- join_by_link is granted only where a realm's caller uses its links up, as
-- staff's does; admins' runs only through redeem_invite. The database
-- cannot check a passkey's signature, so a caller with this role can start a
-- session through login and add a passkey through an addition that session
-- took, for any account. Database access stays the trust boundary (ADR-0061).
GRANT EXECUTE ON FUNCTION identity.use_session(bytea, text),
identity.session_left(bytea, text), identity.describe_session(bytea, text),
identity.log_out(bytea, text), identity.begin_login(bytea, jsonb),
identity.take_ceremony(bytea, text), identity.signer(bytea),
identity.login(bytea, bigint, jsonb, jsonb, bytea, bytea),
identity.account(bytea), identity.sessions_of(bytea),
identity.end_other_sessions(bytea, uuid), identity.adder(bytea),
identity.begin_join(bytea, bigint, bytea, jsonb),
identity.begin_addition(bytea, bytea, bigint, jsonb),
identity.take_addition(bytea, bytea),
identity.add_passkey(bytea, bytea, bigint, bytea, jsonb, text)
TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION identity.start_session(bytea, text, bigint, boolean)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.use_session(bytea, text)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.session_left(bytea, text)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.describe_session(bytea, text)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.log_out(bytea, text) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.holding(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.joining(bigint) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.begin_join(bytea, bigint, bytea, jsonb)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.join_by_link(
    bytea, bigint, bytea, jsonb, text, bytea, bytea
)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.begin_login(bytea, jsonb)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.take_ceremony(bytea, text)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.signer(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.login(bytea, bigint, jsonb, jsonb, bytea, bytea)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.account(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.sessions_of(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.end_other_sessions(bytea, uuid)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.adder(bytea) OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.begin_addition(bytea, bytea, bigint, jsonb)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.take_addition(bytea, bytea)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.add_passkey(bytea, bytea, bigint, bytea, jsonb, text)
OWNER TO tetsu_identity_lookup;
ALTER FUNCTION identity.end_every_session(bigint, timestamptz)
OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
