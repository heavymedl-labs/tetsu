-- How long a live session has left, read without counting as a request, so a
-- page can warn before the idle limit ends it (docs/research/identity/
-- operator-lifecycles.md). Only the person's own requests renew a session:
-- NIST SP 800-63B-4 counts the subscriber's activity, and OWASP warns that a
-- page renewing itself keeps a stolen session alive.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- Whole seconds to each limit on the live session a token's hash names in a
-- realm, by the database's clock, as use_session reads it: none for a session
-- spent, of another realm, or of a disabled operator. It changes and removes
-- nothing; use_session removes a spent session when it is next presented. Its
-- idle limit must match use_session's, which a test checks; #142 gives the
-- limits one home.
CREATE FUNCTION session_left(wanted_hash bytea, wanted_realm text)
RETURNS TABLE (idle_left bigint, absolute_left bigint)
LANGUAGE sql VOLATILE SECURITY DEFINER
SET search_path = pg_catalog, pg_temp
AS $$
    WITH clock AS (
        SELECT
            clock_timestamp() AS at,
            CASE wanted_realm WHEN 'operator' THEN interval '15 minutes' END
                AS idle
    )

    SELECT
        floor(
            extract(EPOCH FROM s.last_seen_at + clock.idle - clock.at)
        )::bigint,
        floor(extract(EPOCH FROM s.ends_at - clock.at))::bigint
    FROM identity.sessions AS s
    INNER JOIN identity.operators AS o ON s.operator_id = o.id
    CROSS JOIN clock
    WHERE
        s.token_hash = wanted_hash
        AND s.realm = wanted_realm
        AND clock.at < s.ends_at
        AND clock.at < s.last_seen_at + clock.idle
        AND o.disabled_at IS NULL
$$;

REVOKE EXECUTE ON FUNCTION session_left(bytea, text) FROM public;
GRANT EXECUTE ON FUNCTION session_left(bytea, text) TO tetsu_identity;

-- A new owner needs CREATE on the schema at the moment it takes a function,
-- and no longer after.
GRANT CREATE ON SCHEMA identity TO tetsu_identity_lookup;
ALTER FUNCTION session_left(bytea, text) OWNER TO tetsu_identity_lookup;
REVOKE CREATE ON SCHEMA identity FROM tetsu_identity_lookup;
