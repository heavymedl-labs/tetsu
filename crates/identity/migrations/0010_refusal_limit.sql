-- Limits on recording refused requests (ADR-0081, #152): a request refused at
-- a check is recorded only while its network has room, and past that, once a
-- window, a record says the network's later refusals go unrecorded.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- The new kinds, checked for new rows at once and for old ones by 0011, so
-- the table is never locked against writes while it is scanned. Dropping the
-- old check, the one way to widen it, is the maintainer's one exception to the
-- migration lint: the table holds only rate-limit counters, which only
-- Identity's own functions read and write, so nothing else relies on the old
-- list of kinds.
-- squawk-ignore ban-drop-constraint
ALTER TABLE limits DROP CONSTRAINT limits_kind_check;
ALTER TABLE limits ADD CONSTRAINT limits_kind_check CHECK (
    kind IN (
        'login_begin', 'login_failure', 'join', 'refusal', 'refusal_flood'
    )
) NOT VALID;

-- Each limit's numbers: the refusals each network may have recorded, ten in
-- fifteen minutes, and the one record a window that later ones are not, then
-- 0004's own.
CREATE OR REPLACE FUNCTION limit_rule(
    wanted_kind text,
    OUT span interval,
    OUT room integer,
    OUT first_wait interval,
    OUT longest interval
)
LANGUAGE sql IMMUTABLE
SET search_path = pg_catalog, pg_temp
AS $$
    WITH rules (kind, span, room, first_wait, longest) AS (
        VALUES
        ('refusal', interval '15 minutes', 10, NULL::interval, NULL::interval),
        ('refusal_flood', interval '15 minutes', 1, NULL, NULL),
        ('join', interval '1 hour', 10, NULL, NULL),
        ('login_failure', interval '15 minutes', 10, interval '1 minute',
            interval '1 hour'),
        ('login_begin', interval '15 minutes', 30, NULL, NULL)
    )

    SELECT rules.span, rules.room, rules.first_wait, rules.longest
    FROM rules
    WHERE rules.kind = wanted_kind
$$;
