-- Every way the catalog breaks ADR-0042's rules, one row per problem.
-- $1 is the modules' schemas, $2 the application role. Each module's record
-- of applied files is the runner's own, so it is left out of the table rules,
-- though not out of the rules on rights or ownership. Anything granted to
-- PUBLIC counts as the application role's own, since it holds it too.
WITH objects AS (
    SELECT
        c.oid,
        c.relname,
        c.relkind,
        c.relacl,
        c.relispartition,
        c.relowner,
        n.nspowner,
        n.nspname AS schema_name,
        coalesce(c.reloptions, '{}') AS rel_options,
        format('%I.%I', n.nspname, c.relname) AS shown
    FROM pg_catalog.pg_class AS c
    INNER JOIN pg_catalog.pg_namespace AS n ON c.relnamespace = n.oid
    WHERE n.nspname = any($1)
),

tables AS (
    SELECT
        o.oid,
        o.schema_name,
        o.shown,
        c.relrowsecurity,
        c.relforcerowsecurity,
        c.relowner,
        coalesce(pg_catalog.obj_description(o.oid, 'pg_class'), '') AS declared
    FROM objects AS o
    INNER JOIN pg_catalog.pg_class AS c ON o.oid = c.oid
    WHERE
        o.relkind IN ('r', 'p')
        AND NOT (o.relname = 'migrations' AND NOT o.relispartition)
),

tenant_owned AS (
    SELECT
        t.*,
        a.attnum AS tenant_column,
        pg_catalog.format('tetsu_%s', t.schema_name) AS module_role
    FROM tables AS t
    LEFT JOIN pg_catalog.pg_attribute AS a
        ON t.oid = a.attrelid AND a.attname = 'tenant_id' AND NOT a.attisdropped
    WHERE t.declared = 'tetsu:tenant-owned'
),

app AS (
    SELECT
        oid,
        rolname,
        rolsuper,
        rolbypassrls,
        rolcreaterole,
        rolcreatedb,
        rolreplication
    FROM pg_catalog.pg_roles
    WHERE rolname = $2
),

-- Every role the application role can reach with SET ROLE, however many
-- grants away.
acting AS (
    SELECT
        r.oid,
        r.rolname,
        r.rolsuper,
        r.rolbypassrls,
        r.rolcreaterole,
        r.rolcreatedb,
        r.rolreplication
    FROM pg_catalog.pg_roles AS r
    WHERE r.rolname <> $2 AND pg_catalog.pg_has_role($2, r.oid, 'SET')
),

-- Rights on whole objects and on single columns alike.
object_grants AS (
    SELECT
        o.shown,
        o.schema_name,
        o.relispartition,
        g.grantee,
        g.privilege_type
    FROM objects AS o
    CROSS JOIN LATERAL pg_catalog.aclexplode(o.relacl) AS g
    UNION ALL
    SELECT
        o.shown || ' (column ' || a.attname || ')' AS shown,
        o.schema_name,
        o.relispartition,
        g.grantee,
        g.privilege_type
    FROM objects AS o
    INNER JOIN pg_catalog.pg_attribute AS a ON o.oid = a.attrelid
    CROSS JOIN LATERAL pg_catalog.aclexplode(a.attacl) AS g
    WHERE NOT a.attisdropped
),

schema_grants AS (
    SELECT
        n.nspname AS schema_name,
        g.grantee,
        g.privilege_type
    FROM pg_catalog.pg_namespace AS n
    CROSS JOIN
        LATERAL pg_catalog.aclexplode(
            coalesce(n.nspacl, pg_catalog.acldefault('n', n.nspowner))
        ) AS g
    WHERE n.nspname = any($1)
),

-- A function with no grants of its own may be run by PUBLIC.
function_grants AS (
    SELECT
        n.nspname AS schema_name,
        p.prosecdef,
        g.grantee,
        g.privilege_type,
        p.oid::pg_catalog.regprocedure::text AS shown
    FROM pg_catalog.pg_proc AS p
    INNER JOIN pg_catalog.pg_namespace AS n ON p.pronamespace = n.oid
    CROSS JOIN
        LATERAL pg_catalog.aclexplode(
            coalesce(p.proacl, pg_catalog.acldefault('f', p.proowner))
        ) AS g
    WHERE n.nspname = any($1)
),

definers AS (
    SELECT
        n.nspname AS schema_name,
        p.oid::pg_catalog.regprocedure::text AS shown,
        p.proowner::pg_catalog.regrole::text AS owner_name,
        coalesce(p.proconfig, '{}') AS config
    FROM pg_catalog.pg_proc AS p
    INNER JOIN pg_catalog.pg_namespace AS n ON p.pronamespace = n.oid
    WHERE n.nspname = any($1) AND p.prosecdef
),

-- The application role, and PUBLIC, which it belongs to, named for messages.
holders AS (
    SELECT
        oid,
        rolname AS name
    FROM app
    UNION ALL
    SELECT
        0 AS oid,
        'PUBLIC' AS name
),

owners AS (
    SELECT DISTINCT o.relowner AS oid
    FROM objects AS o
    UNION
    SELECT nspowner AS oid
    FROM pg_catalog.pg_namespace
    WHERE nspname = any($1)
)

SELECT t.shown || ' is declared neither tenant-owned nor global' AS problem
FROM tables AS t
WHERE t.declared <> 'tetsu:tenant-owned' AND t.declared !~ '^tetsu:global: .+'

UNION ALL
SELECT t.shown || ' lacks row-level security, enabled and forced'
FROM tenant_owned AS t
WHERE NOT (t.relrowsecurity AND t.relforcerowsecurity)

UNION ALL
SELECT t.shown || ' has no tenant_id column'
FROM tenant_owned AS t
WHERE t.tenant_column IS NULL

UNION ALL
SELECT
    t.shown
    || ' lacks a '
    || kind.name
    || ' tenant policy for '
    || t.module_role
FROM tenant_owned AS t
CROSS JOIN
    (VALUES (TRUE, 'permissive'), (FALSE, 'restrictive'))
        AS kind (permissive, name)
WHERE
    NOT EXISTS (
        SELECT FROM pg_catalog.pg_policy AS p
        INNER JOIN pg_catalog.pg_roles AS r ON r.rolname = t.module_role
        WHERE
            p.polrelid = t.oid
            AND p.polpermissive = kind.permissive
            AND r.oid = any(p.polroles)
            AND p.polcmd = '*'
            AND p.polwithcheck IS NULL
            AND pg_catalog.pg_get_expr(p.polqual, p.polrelid)
            -- ADR-0042's rule, as Postgres prints it back.
            = '(tenant_id = ( SELECT (NULLIF(current_setting('
            || '''app.tenant_id''::text, true), ''''::text))::bigint'
            || ' AS "nullif"))'
    )

UNION ALL
SELECT
    format('%I.%I', n.nspname, c.relname)
    || ' has policy '
    || p.polname
    || ' for every role'
FROM pg_catalog.pg_policy AS p
INNER JOIN pg_catalog.pg_class AS c ON p.polrelid = c.oid
INNER JOIN pg_catalog.pg_namespace AS n ON c.relnamespace = n.oid
WHERE n.nspname = any($1) AND 0 = any(p.polroles)

UNION ALL
SELECT
    format('%I.%I', n.nspname, c.relname)
    || ' has policy '
    || p.polname
    || ' for its owner, '
    || c.relowner::pg_catalog.regrole::text
FROM pg_catalog.pg_policy AS p
INNER JOIN pg_catalog.pg_class AS c ON p.polrelid = c.oid
INNER JOIN pg_catalog.pg_namespace AS n ON c.relnamespace = n.oid
WHERE n.nspname = any($1) AND c.relowner = any(p.polroles)

UNION ALL
SELECT t.shown || '''s primary key does not start with tenant_id'
FROM tenant_owned AS t
WHERE
    NOT EXISTS (
        SELECT FROM pg_catalog.pg_constraint AS k
        WHERE
            k.conrelid = t.oid
            AND k.contype = 'p'
            AND k.conkey[1] = t.tenant_column
    )

UNION ALL
SELECT t.shown || '''s foreign key ' || k.conname || ' leaves out tenant_id'
FROM tenant_owned AS t
INNER JOIN
    pg_catalog.pg_constraint AS k
    ON t.oid = k.conrelid AND k.contype = 'f'
WHERE NOT (t.tenant_column = any(k.conkey))

UNION ALL
SELECT
    t.shown
    || '''s unique index '
    || i.indexrelid::pg_catalog.regclass::text
    || ' does not start with tenant_id'
FROM tenant_owned AS t
INNER JOIN pg_catalog.pg_index AS i ON t.oid = i.indrelid AND i.indisunique
WHERE i.indkey[0] IS DISTINCT FROM t.tenant_column

UNION ALL
SELECT
    t.shown
    || '''s exclusion constraint '
    || k.conname
    || ' does not start with tenant_id'
FROM tenant_owned AS t
INNER JOIN
    pg_catalog.pg_constraint AS k
    ON t.oid = k.conrelid AND k.contype = 'x'
WHERE k.conkey[1] IS DISTINCT FROM t.tenant_column

UNION ALL
SELECT
    o.shown
    || ' is owned by '
    || o.relowner::pg_catalog.regrole::text
    || ', not its schema''s owner'
FROM objects AS o
WHERE o.relowner <> o.nspowner

UNION ALL
SELECT o.shown || ' is a view that runs with its owner''s rights'
FROM objects AS o
WHERE o.relkind = 'v' AND NOT 'security_invoker=true' = any(o.rel_options)

UNION ALL
SELECT
    o.shown
    || ' is a materialized view, which row-level security does not reach'
FROM objects AS o
WHERE o.relkind = 'm'

UNION ALL
SELECT
    h.name
    || ' holds '
    || g.privilege_type
    || ' on '
    || g.shown
    || ' of its own'
FROM object_grants AS g
INNER JOIN holders AS h ON g.grantee = h.oid

UNION ALL
SELECT
    h.name
    || ' holds '
    || g.privilege_type
    || ' on schema '
    || g.schema_name
    || ' of its own'
FROM schema_grants AS g
INNER JOIN holders AS h ON g.grantee = h.oid

UNION ALL
SELECT
    h.name
    || ' holds '
    || g.privilege_type
    || ' on function '
    || g.shown
    || ' of its own'
FROM function_grants AS g
INNER JOIN holders AS h ON g.grantee = h.oid
WHERE NOT g.prosecdef

UNION ALL
SELECT
    h.name
    || ' is named in default privileges of '
    || d.defaclrole::pg_catalog.regrole::text
FROM pg_catalog.pg_default_acl AS d
CROSS JOIN LATERAL pg_catalog.aclexplode(d.defaclacl) AS g
INNER JOIN holders AS h ON g.grantee = h.oid

UNION ALL
SELECT $2 || ' inherits the rights of ' || m.roleid::pg_catalog.regrole::text
FROM pg_catalog.pg_auth_members AS m
INNER JOIN app ON m.member = app.oid
WHERE m.inherit_option

UNION ALL
SELECT $2 || ' may grant ' || m.roleid::pg_catalog.regrole::text
FROM pg_catalog.pg_auth_members AS m
INNER JOIN app ON m.member = app.oid
WHERE m.admin_option

UNION ALL
SELECT $2 || ' can act as ' || a.rolname || ', which is no module''s role'
FROM acting AS a
WHERE
    a.rolname !~ '^tetsu_[a-z_][a-z0-9_]*$'
    OR a.rolname ~ '_lookup$'
    OR a.oid IN (SELECT ow.oid FROM owners AS ow)

UNION ALL
SELECT $2 || ' owns ' || held_objects.described
FROM (
    SELECT
        relowner AS owner_oid,
        oid::pg_catalog.regclass::text AS described
    FROM pg_catalog.pg_class
    UNION ALL
    SELECT
        proowner AS owner_oid,
        'function ' || oid::pg_catalog.regprocedure::text AS described
    FROM pg_catalog.pg_proc
    UNION ALL
    SELECT
        nspowner AS owner_oid,
        'schema ' || nspname AS described
    FROM pg_catalog.pg_namespace
) AS held_objects
INNER JOIN app ON held_objects.owner_oid = app.oid

UNION ALL
SELECT r.rolname || ' skips row-level security'
FROM (
    SELECT * FROM app
    UNION ALL
    SELECT * FROM acting
) AS r
WHERE r.rolsuper OR r.rolbypassrls

UNION ALL
SELECT r.rolname || ' can create roles or databases, or replicate'
FROM (
    SELECT * FROM app
    UNION ALL
    SELECT * FROM acting
) AS r
WHERE r.rolcreaterole OR r.rolcreatedb OR r.rolreplication

-- A superuser owner is allowed, as on a local test server; a managed host's
-- role that skips row-level security without being one is not.
UNION ALL
SELECT 'the owner ' || r.rolname || ' skips row-level security'
FROM pg_catalog.pg_roles AS r
WHERE
    r.oid IN (SELECT ow.oid FROM owners AS ow)
    AND r.rolbypassrls
    AND NOT r.rolsuper

UNION ALL
SELECT a.rolname || ' holds TRUNCATE on ' || g.shown
FROM object_grants AS g
INNER JOIN acting AS a ON g.grantee = a.oid
WHERE g.privilege_type = 'TRUNCATE'

UNION ALL
SELECT a.rolname || ' holds ' || g.privilege_type || ' on partition ' || g.shown
FROM object_grants AS g
INNER JOIN acting AS a ON g.grantee = a.oid
WHERE g.relispartition

UNION ALL
SELECT
    a.rolname
    || ' holds '
    || g.privilege_type
    || ' on '
    || g.shown
    || ', in another module''s schema'
FROM object_grants AS g
INNER JOIN acting AS a ON g.grantee = a.oid
WHERE a.rolname <> pg_catalog.format('tetsu_%s', g.schema_name)

UNION ALL
SELECT
    a.rolname
    || ' holds '
    || g.privilege_type
    || ' on schema '
    || g.schema_name
    || ', another module''s'
FROM schema_grants AS g
INNER JOIN acting AS a ON g.grantee = a.oid
WHERE a.rolname <> pg_catalog.format('tetsu_%s', g.schema_name)

UNION ALL
SELECT a.rolname || ' holds CREATE on schema ' || g.schema_name
FROM schema_grants AS g
INNER JOIN acting AS a ON g.grantee = a.oid
WHERE g.privilege_type = 'CREATE'

UNION ALL
SELECT
    a.rolname
    || ' holds EXECUTE on function '
    || g.shown
    || ', in another module''s schema'
FROM function_grants AS g
INNER JOIN acting AS a ON g.grantee = a.oid
WHERE a.rolname <> pg_catalog.format('tetsu_%s', g.schema_name)

UNION ALL
SELECT
    'database settings for '
    || CASE
        WHEN s.setrole = 0 THEN 'every role'
        ELSE s.setrole::pg_catalog.regrole::text
    END
    || ' set '
    || split_part(setting.value, '=', 1)
FROM pg_catalog.pg_db_role_setting AS s
CROSS JOIN LATERAL unnest(s.setconfig) AS setting (value)
WHERE
    (s.setdatabase = 0 OR s.setdatabase IN (
        SELECT d.oid FROM pg_catalog.pg_database AS d
        WHERE d.datname = current_database()
    ))
    AND (s.setrole = 0 OR s.setrole IN (SELECT ap.oid FROM app AS ap))
    AND (
        split_part(setting.value, '=', 1)
        IN ('role', 'search_path', 'session_authorization')
        OR split_part(setting.value, '=', 1) LIKE 'app.%'
    )

UNION ALL
SELECT
    'function '
    || d.shown
    || ' runs as its owner but is not owned by '
    || pg_catalog.format('tetsu_%s_lookup', d.schema_name)
FROM definers AS d
WHERE d.owner_name <> pg_catalog.format('tetsu_%s_lookup', d.schema_name)

UNION ALL
SELECT
    'function '
    || d.shown
    || ' runs as its owner but sets no search_path of pg_catalog, pg_temp'
FROM definers AS d
WHERE NOT 'search_path=pg_catalog, pg_temp' = any(d.config)

UNION ALL
SELECT 'function ' || g.shown || ' runs as its owner but anyone may run it'
FROM function_grants AS g
WHERE g.prosecdef AND g.grantee = 0 AND g.privilege_type = 'EXECUTE'

ORDER BY 1;
