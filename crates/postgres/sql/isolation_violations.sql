-- Every way the catalog breaks ADR-0042's rules, one row per problem.
-- $1 is the modules' schemas, $2 the application role. Each module's record
-- of applied files is the runner's own, so it is left out of the table rules,
-- though not out of the rules on rights.
WITH objects AS (
    SELECT
        c.oid,
        c.relacl,
        c.relispartition,
        n.nspname AS schema_name,
        format('%I.%I', n.nspname, c.relname) AS shown
    FROM pg_catalog.pg_class AS c
    INNER JOIN pg_catalog.pg_namespace AS n ON c.relnamespace = n.oid
    WHERE n.nspname = any($1)
),

tables AS (
    SELECT
        c.oid,
        n.nspname AS schema_name,
        c.relname AS name,
        c.relrowsecurity,
        c.relforcerowsecurity,
        c.relispartition,
        c.relowner,
        n.nspowner,
        format('%I.%I', n.nspname, c.relname) AS shown,
        coalesce(pg_catalog.obj_description(c.oid, 'pg_class'), '') AS declared
    FROM pg_catalog.pg_class AS c
    INNER JOIN pg_catalog.pg_namespace AS n ON c.relnamespace = n.oid
    WHERE
        n.nspname = any($1)
        AND c.relkind IN ('r', 'p')
        AND NOT (c.relname = 'migrations' AND NOT c.relispartition)
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
        rolsuper,
        rolbypassrls
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
        r.rolbypassrls
    FROM pg_catalog.pg_roles AS r
    WHERE r.rolname <> $2 AND pg_catalog.pg_has_role($2, r.oid, 'SET')
),

table_grants AS (
    SELECT
        o.shown,
        o.schema_name,
        o.relispartition,
        g.grantee,
        g.privilege_type
    FROM objects AS o
    CROSS JOIN LATERAL pg_catalog.aclexplode(o.relacl) AS g
),

definers AS (
    SELECT
        p.proacl,
        n.nspname AS schema_name,
        p.oid::pg_catalog.regprocedure::text AS shown,
        p.proowner::pg_catalog.regrole::text AS owner_name,
        coalesce(p.proconfig, '{}') AS config
    FROM pg_catalog.pg_proc AS p
    INNER JOIN pg_catalog.pg_namespace AS n ON p.pronamespace = n.oid
    WHERE n.nspname = any($1) AND p.prosecdef
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
    || ' is owned by '
    || t.relowner::pg_catalog.regrole::text
    || ', not its schema''s owner'
FROM tables AS t
WHERE t.relowner <> t.nspowner

UNION ALL
SELECT $2 || ' holds ' || g.privilege_type || ' on ' || g.shown || ' of its own'
FROM table_grants AS g
INNER JOIN app ON g.grantee = app.oid

UNION ALL
SELECT
    $2
    || ' holds '
    || g.privilege_type
    || ' on schema '
    || n.nspname
    || ' of its own'
FROM pg_catalog.pg_namespace AS n
CROSS JOIN LATERAL pg_catalog.aclexplode(n.nspacl) AS g
INNER JOIN app ON g.grantee = app.oid
WHERE n.nspname = any($1)

UNION ALL
SELECT
    $2
    || ' holds '
    || g.privilege_type
    || ' on function '
    || p.oid::pg_catalog.regprocedure::text
    || ' of its own'
FROM pg_catalog.pg_proc AS p
INNER JOIN pg_catalog.pg_namespace AS n ON p.pronamespace = n.oid
CROSS JOIN LATERAL pg_catalog.aclexplode(p.proacl) AS g
INNER JOIN app ON g.grantee = app.oid
WHERE n.nspname = any($1)

UNION ALL
SELECT
    $2
    || ' is named in default privileges of '
    || d.defaclrole::pg_catalog.regrole::text
FROM pg_catalog.pg_default_acl AS d
CROSS JOIN LATERAL pg_catalog.aclexplode(d.defaclacl) AS g
INNER JOIN app ON g.grantee = app.oid

UNION ALL
SELECT $2 || ' inherits the rights of ' || m.roleid::pg_catalog.regrole::text
FROM pg_catalog.pg_auth_members AS m
INNER JOIN app ON m.member = app.oid
WHERE m.inherit_option

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
SELECT r.name || ' skips row-level security'
FROM (
    SELECT
        $2 AS name,
        rolsuper,
        rolbypassrls
    FROM app
    UNION ALL
    SELECT
        rolname,
        rolsuper,
        rolbypassrls
    FROM acting
) AS r
WHERE r.rolsuper OR r.rolbypassrls

UNION ALL
SELECT a.rolname || ' holds TRUNCATE on ' || g.shown
FROM table_grants AS g
INNER JOIN acting AS a ON g.grantee = a.oid
WHERE g.privilege_type = 'TRUNCATE'

UNION ALL
SELECT a.rolname || ' holds ' || g.privilege_type || ' on partition ' || g.shown
FROM table_grants AS g
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
FROM table_grants AS g
INNER JOIN acting AS a ON g.grantee = a.oid
WHERE a.rolname <> pg_catalog.format('tetsu_%s', g.schema_name)

UNION ALL
SELECT
    'function '
    || d.shown
    || ' runs as its owner but is not owned by '
    || pg_catalog.format('tetsu_%s_lookup', d.schema_name)
FROM definers AS d
WHERE d.owner_name <> pg_catalog.format('tetsu_%s_lookup', d.schema_name)

UNION ALL
SELECT 'function ' || d.shown || ' runs as its owner but sets no search_path'
FROM definers AS d
WHERE
    NOT EXISTS (
        SELECT FROM unnest(d.config) AS s
        WHERE s LIKE 'search_path=%'
    )

UNION ALL
SELECT 'function ' || d.shown || ' runs as its owner but anyone may run it'
FROM definers AS d
WHERE
    d.proacl IS NULL OR EXISTS (
        SELECT FROM pg_catalog.aclexplode(d.proacl) AS g
        WHERE g.grantee = 0 AND g.privilege_type = 'EXECUTE'
    )

ORDER BY 1;
