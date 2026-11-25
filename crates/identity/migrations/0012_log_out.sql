-- Logging out, recorded (#167, v5.0.0-16.3.1): ending a session by choice
-- says which session and account it was, so the audit record can name them,
-- as 0001's end_session, which says only whether one was live, cannot. It
-- replaces end_session, which the application may no longer run, so no way to
-- end a session goes unrecorded; an image older than this one fails to log
-- anyone out until it is replaced, which Tetsu accepts while it holds no
-- business's data.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- Removes a session in a realm, and says whether it was live when it ended,
-- with its public id and account: a spent one is removed too, but it had
-- already ended. No row means there was none.
CREATE FUNCTION log_out(wanted_hash bytea, wanted_realm text)
RETURNS TABLE (live boolean, session_id uuid, account_id bigint)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH clock AS (
        SELECT
            clock_timestamp() AS at,
            CASE wanted_realm WHEN 'admin' THEN interval '15 minutes' END
                AS idle
    )

    DELETE FROM identity.sessions AS s
    USING clock
    WHERE s.token_hash = wanted_hash AND s.realm = wanted_realm
    RETURNING
        coalesce(clock.at < s.ends_at AND clock.at < s.last_seen_at + clock.idle,
            false),
        s.id,
        s.admin_id
$$;

REVOKE EXECUTE ON FUNCTION end_session(bytea, text) FROM tetsu_identity;

REVOKE EXECUTE ON FUNCTION log_out(bytea, text) FROM public;
GRANT EXECUTE ON FUNCTION log_out(bytea, text) TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION log_out(bytea, text) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
