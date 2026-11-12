-- Every way the catalog departs from ADR-0042, one row per problem. $1 is the
-- modules' schemas and $2 the application role.
--
-- It is an allowlist: it describes what a correct database holds and reports
-- anything else. Each module schema is owned by the owner role, which owns
-- everything in it except lookup functions, owned by the module's lookup
-- role. The module role tetsu_<schema> and the lookup role
-- tetsu_<schema>_lookup hold only their own schema's rights, belong to no role,
-- and hold no attribute. The application role holds nothing of its own, and
-- belongs only to module roles, which it may act as but not inherit. Anything
-- granted to PUBLIC counts as the application role's own. A module's record of
-- applied files is the runner's own, so it is left out of the table rules.
WITH modules AS (
    SELECT
        n.oid,
        n.nspname AS schema_name,
        n.nspowner,
        pg_catalog.format('tetsu_%s', n.nspname) AS module_role,
        pg_catalog.format('tetsu_%s_lookup', n.nspname) AS lookup_role
    FROM pg_catalog.pg_namespace AS n
    WHERE n.nspname = any($1)
),

app AS (
    SELECT
        r.oid,
        r.rolname,
        r.rolsuper,
        r.rolbypassrls,
        r.rolcreaterole,
        r.rolcreatedb,
        r.rolreplication,
        FALSE AS rolcanlogin
    FROM pg_catalog.pg_roles AS r
    WHERE r.rolname = $2
),

-- Every role the application role belongs to, however many grants away and
-- whatever their options.
reached AS (
    SELECT
        r.oid,
        r.rolname
    FROM pg_catalog.pg_roles AS r
    WHERE r.rolname <> $2 AND pg_catalog.pg_has_role($2, r.oid, 'MEMBER')
),

-- The roles that must hold no attribute and belong to nothing.
bound_roles AS (
    SELECT
        r.oid,
        r.rolname,
        r.rolsuper,
        r.rolbypassrls,
        r.rolcreaterole,
        r.rolcreatedb,
        r.rolreplication,
        r.rolcanlogin
    FROM pg_catalog.pg_roles AS r
    WHERE
        r.oid IN (SELECT rc.oid FROM reached AS rc)
        OR r.rolname IN (SELECT m.module_role FROM modules AS m)
        OR r.rolname IN (SELECT m.lookup_role FROM modules AS m)
),

owners AS (
    SELECT DISTINCT m.nspowner AS oid
    FROM modules AS m
),

-- Every object in a module schema, with its owner.
owned AS (
    SELECT
        m.nspowner,
        c.relowner AS owner_oid,
        FALSE AS definer,
        pg_catalog.format('%I.%I', m.schema_name, c.relname) AS shown
    FROM pg_catalog.pg_class AS c
    INNER JOIN modules AS m ON c.relnamespace = m.oid
    UNION ALL
    SELECT
        m.nspowner,
        p.proowner AS owner_oid,
        p.prosecdef AS definer,
        'function ' || p.oid::pg_catalog.regprocedure::text AS shown
    FROM pg_catalog.pg_proc AS p
    INNER JOIN modules AS m ON p.pronamespace = m.oid
    UNION ALL
    SELECT
        m.nspowner,
        t.typowner AS owner_oid,
        FALSE AS definer,
        'type ' || t.oid::pg_catalog.regtype::text AS shown
    FROM pg_catalog.pg_type AS t
    INNER JOIN modules AS m ON t.typnamespace = m.oid
    UNION ALL
    SELECT
        m.nspowner,
        o.oprowner AS owner_oid,
        FALSE AS definer,
        'operator ' || o.oid::pg_catalog.regoperator::text AS shown
    FROM pg_catalog.pg_operator AS o
    INNER JOIN modules AS m ON o.oprnamespace = m.oid
    UNION ALL
    SELECT
        m.nspowner,
        x.stxowner AS owner_oid,
        FALSE AS definer,
        'statistics ' || x.stxname AS shown
    FROM pg_catalog.pg_statistic_ext AS x
    INNER JOIN modules AS m ON x.stxnamespace = m.oid
),

relations AS (
    SELECT
        c.oid,
        c.relname,
        c.relkind,
        c.relacl,
        c.reloptions,
        c.relispartition,
        c.relrowsecurity,
        c.relforcerowsecurity,
        c.relowner,
        m.schema_name,
        m.module_role,
        pg_catalog.format('%I.%I', m.schema_name, c.relname) AS shown,
        coalesce(pg_catalog.obj_description(c.oid, 'pg_class'), '') AS declared
    FROM pg_catalog.pg_class AS c
    INNER JOIN modules AS m ON c.relnamespace = m.oid
),

tables AS (
    SELECT r.*
    FROM relations AS r
    WHERE
        r.relkind IN ('r', 'p')
        AND NOT (r.relname = 'migrations' AND NOT r.relispartition)
),

tenant_owned AS (
    SELECT
        t.*,
        a.attnum AS tenant_column,
        ARRAY[(
            SELECT r.oid FROM pg_catalog.pg_roles AS r
            WHERE r.rolname = t.module_role
        )] AS module_roles
    FROM tables AS t
    LEFT JOIN pg_catalog.pg_attribute AS a
        ON t.oid = a.attrelid AND a.attname = 'tenant_id' AND NOT a.attisdropped
    WHERE t.declared = 'tetsu:tenant-owned'
),

-- ADR-0042's rule, as Postgres prints it back.
tenant_rule AS (
    SELECT
        '(tenant_id = ( SELECT (NULLIF(current_setting('
        || '''app.tenant_id''::text, true), ''''::text))::bigint'
        || ' AS "nullif"))' AS expected
),

-- Every right granted on anything in a module schema, as kind, object,
-- grantee, and privilege. A missing access list means the kind's default.
grants AS (
    SELECT
        r.schema_name,
        CASE
            WHEN r.relispartition THEN 'partition'
            WHEN r.relkind = 'S' THEN 'sequence'
            ELSE 'table'
        END AS kind,
        CASE
            WHEN r.relispartition THEN 'partition ' || r.shown
            ELSE r.shown
        END AS shown,
        FALSE AS definer,
        g.grantee,
        g.privilege_type
    FROM relations AS r
    CROSS JOIN LATERAL pg_catalog.aclexplode(r.relacl) AS g
    UNION ALL
    SELECT
        r.schema_name,
        CASE WHEN r.relispartition THEN 'partition' ELSE 'table' END AS kind,
        r.shown || ' (column ' || a.attname || ')' AS shown,
        FALSE AS definer,
        g.grantee,
        g.privilege_type
    FROM relations AS r
    INNER JOIN pg_catalog.pg_attribute AS a ON r.oid = a.attrelid
    CROSS JOIN LATERAL pg_catalog.aclexplode(a.attacl) AS g
    WHERE NOT a.attisdropped
    UNION ALL
    SELECT
        m.schema_name,
        'schema' AS kind,
        'schema ' || m.schema_name AS shown,
        FALSE AS definer,
        g.grantee,
        g.privilege_type
    FROM modules AS m
    INNER JOIN pg_catalog.pg_namespace AS n ON m.oid = n.oid
    CROSS JOIN
        LATERAL pg_catalog.aclexplode(
            coalesce(n.nspacl, pg_catalog.acldefault('n', n.nspowner))
        ) AS g
    UNION ALL
    SELECT
        m.schema_name,
        'function' AS kind,
        'function ' || p.oid::pg_catalog.regprocedure::text AS shown,
        p.prosecdef AS definer,
        g.grantee,
        g.privilege_type
    FROM modules AS m
    INNER JOIN pg_catalog.pg_proc AS p ON m.oid = p.pronamespace
    CROSS JOIN
        LATERAL pg_catalog.aclexplode(
            coalesce(p.proacl, pg_catalog.acldefault('f', p.proowner))
        ) AS g
    UNION ALL
    SELECT
        m.schema_name,
        'type' AS kind,
        'type ' || t.oid::pg_catalog.regtype::text AS shown,
        FALSE AS definer,
        g.grantee,
        g.privilege_type
    FROM modules AS m
    INNER JOIN pg_catalog.pg_type AS t ON m.oid = t.typnamespace
    CROSS JOIN
        LATERAL pg_catalog.aclexplode(
            coalesce(t.typacl, pg_catalog.acldefault('T', t.typowner))
        ) AS g
),

-- What each right may be: the owner holds anything, the module's own roles
-- hold what their work needs, and PUBLIC may only use types.
judged AS (
    SELECT
        g.*,
        coalesce(gr.rolname, 'PUBLIC') AS grantee_name,
        -- A NULL, as for PUBLIC, which names no role, is not allowed.
        coalesce((
            g.grantee IN (SELECT o.oid FROM owners AS o)
            OR (
                gr.rolname IN (
                    pg_catalog.format('tetsu_%s', g.schema_name),
                    pg_catalog.format('tetsu_%s_lookup', g.schema_name)
                )
                AND (
                    (
                        g.kind = 'table'
                        AND g.privilege_type
                        IN ('SELECT', 'INSERT', 'UPDATE', 'DELETE')
                    )
                    OR (
                        g.kind = 'sequence'
                        AND g.privilege_type IN ('USAGE', 'SELECT')
                    )
                    OR (g.kind = 'schema' AND g.privilege_type = 'USAGE')
                    OR (g.kind = 'function' AND g.privilege_type = 'EXECUTE')
                    OR (g.kind = 'type' AND g.privilege_type = 'USAGE')
                )
            )
            OR (
                g.grantee = 0 AND g.kind = 'type' AND g.privilege_type = 'USAGE'
            )
        ), FALSE) AS allowed,
        gr.rolname IS NOT NULL
        AND gr.rolname ~ '^tetsu_'
        AND gr.rolname NOT IN (
            pg_catalog.format('tetsu_%s', g.schema_name),
            pg_catalog.format('tetsu_%s_lookup', g.schema_name)
        ) AS foreign_module
    FROM grants AS g
    LEFT JOIN pg_catalog.pg_roles AS gr ON g.grantee = gr.oid
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
        CROSS JOIN tenant_rule AS tr
        WHERE
            p.polrelid = t.oid
            AND p.polpermissive = kind.permissive
            AND p.polroles = t.module_roles
            AND p.polcmd = '*'
            AND p.polwithcheck IS NULL
            AND pg_catalog.pg_get_expr(p.polqual, p.polrelid) = tr.expected
    )

UNION ALL
SELECT
    t.shown
    || ' has policy '
    || p.polname
    || ' that is not the tenant rule for '
    || t.module_role
    || ' alone'
FROM tenant_owned AS t
INNER JOIN pg_catalog.pg_policy AS p ON t.oid = p.polrelid
CROSS JOIN tenant_rule AS tr
WHERE
    p.polroles <> t.module_roles
    OR p.polcmd <> '*'
    OR p.polwithcheck IS NOT NULL
    OR pg_catalog.pg_get_expr(p.polqual, p.polrelid) <> tr.expected

UNION ALL
SELECT r.shown || ' has policy ' || p.polname || ' for every role'
FROM relations AS r
INNER JOIN pg_catalog.pg_policy AS p ON r.oid = p.polrelid
WHERE 0 = any(p.polroles)

UNION ALL
SELECT
    r.shown
    || ' has policy '
    || p.polname
    || ' for its owner, '
    || r.relowner::pg_catalog.regrole::text
FROM relations AS r
INNER JOIN pg_catalog.pg_policy AS p ON r.oid = p.polrelid
WHERE
    r.relowner = any(p.polroles)
    OR (
        -- A superuser belongs to every role, and skips row-level security
        -- anyway, so only a direct grant counts for one.
        NOT (
            SELECT ro.rolsuper FROM pg_catalog.pg_roles AS ro
            WHERE ro.oid = r.relowner
        )
        AND EXISTS (
            SELECT FROM unnest(p.polroles) AS pr (oid)
            WHERE
                pr.oid <> 0
                AND pg_catalog.pg_has_role(r.relowner, pr.oid, 'USAGE')
        )
    )

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

-- An exclusion constraint must compare tenant_id for equality first, so it
-- never weighs one tenant's rows against another's.
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
WHERE
    k.conkey[1] IS DISTINCT FROM t.tenant_column
    OR k.conexclop[1]
    IS DISTINCT FROM 'pg_catalog.=(int8,int8)'::pg_catalog.regoperator

UNION ALL
SELECT
    o.shown
    || ' is owned by '
    || o.owner_oid::pg_catalog.regrole::text
    || ', not its schema''s owner'
FROM owned AS o
WHERE o.owner_oid <> o.nspowner AND NOT o.definer

UNION ALL
SELECT r.shown || ' is a view that runs with its owner''s rights'
FROM relations AS r
WHERE
    r.relkind = 'v'
    AND NOT EXISTS (
        SELECT FROM pg_catalog.pg_options_to_table(r.reloptions) AS opt
        WHERE
            opt.option_name = 'security_invoker'
            AND opt.option_value::boolean
    )

UNION ALL
SELECT
    r.shown
    || ' is a materialized view, which row-level security does not reach'
FROM relations AS r
WHERE r.relkind = 'm'

UNION ALL
SELECT r.shown || ' is a kind of relation a module schema does not hold'
FROM relations AS r
WHERE r.relkind NOT IN ('r', 'p', 'i', 'I', 'S', 'v', 'm')

UNION ALL
SELECT
    r.shown
    || ' has rule '
    || w.rulename
    || ', which runs with its owner''s rights'
FROM relations AS r
INNER JOIN pg_catalog.pg_rewrite AS w ON r.oid = w.ev_class
WHERE w.rulename <> '_RETURN'

UNION ALL
SELECT
    j.grantee_name
    || ' holds '
    || j.privilege_type
    || ' on '
    || j.shown
    || CASE
        WHEN j.grantee = 0 OR j.grantee IN (SELECT ap.oid FROM app AS ap)
            THEN ' of its own'
        WHEN j.foreign_module AND j.kind = 'schema' THEN ', another module''s'
        WHEN j.foreign_module THEN ', in another module''s schema'
        ELSE ''
    END
FROM judged AS j
WHERE NOT j.allowed AND NOT (j.definer AND j.grantee = 0)

-- Default privileges: within a module schema only the module's own roles,
-- and the owner's global defaults reach no one else, apart from PUBLIC's use
-- of types, which is Postgres's own default. No role's defaults name the
-- application role.
UNION ALL
SELECT
    coalesce(gr.rolname, 'PUBLIC')
    || ' is named in default privileges of '
    || d.defaclrole::pg_catalog.regrole::text
FROM pg_catalog.pg_default_acl AS d
CROSS JOIN LATERAL pg_catalog.aclexplode(d.defaclacl) AS g
LEFT JOIN pg_catalog.pg_roles AS gr ON g.grantee = gr.oid
LEFT JOIN modules AS m ON d.defaclnamespace = m.oid
WHERE
    g.grantee IN (SELECT ap.oid FROM app AS ap)
    OR (
        d.defaclrole IN (SELECT o.oid FROM owners AS o)
        AND g.grantee <> d.defaclrole
        AND NOT (g.grantee = 0 AND d.defaclobjtype = 'T')
        AND NOT (m.oid IS NOT NULL AND gr.rolname = m.module_role)
    )

UNION ALL
SELECT $2 || ' inherits the rights of ' || m.roleid::pg_catalog.regrole::text
FROM pg_catalog.pg_auth_members AS m
INNER JOIN app AS ap ON m.member = ap.oid
WHERE m.inherit_option

UNION ALL
SELECT $2 || ' may grant ' || m.roleid::pg_catalog.regrole::text
FROM pg_catalog.pg_auth_members AS m
INNER JOIN app AS ap ON m.member = ap.oid
WHERE m.admin_option

UNION ALL
SELECT $2 || ' can act as ' || rc.rolname || ', which is no module''s role'
FROM reached AS rc
WHERE
    rc.rolname !~ '^tetsu_[a-z_][a-z0-9_]*$'
    OR rc.rolname ~ '_lookup$'
    OR rc.oid IN (SELECT o.oid FROM owners AS o)

UNION ALL
SELECT
    br.rolname
    || ' belongs to '
    || m.roleid::pg_catalog.regrole::text
    || ', but module and lookup roles belong to nothing'
FROM bound_roles AS br
INNER JOIN pg_catalog.pg_auth_members AS m ON br.oid = m.member

UNION ALL
SELECT $2 || ' owns ' || held.described
FROM (
    SELECT
        c.relowner AS owner_oid,
        c.oid::pg_catalog.regclass::text AS described
    FROM pg_catalog.pg_class AS c
    UNION ALL
    SELECT
        p.proowner AS owner_oid,
        'function ' || p.oid::pg_catalog.regprocedure::text AS described
    FROM pg_catalog.pg_proc AS p
    UNION ALL
    SELECT
        n.nspowner AS owner_oid,
        'schema ' || n.nspname AS described
    FROM pg_catalog.pg_namespace AS n
    UNION ALL
    SELECT
        t.typowner AS owner_oid,
        'type ' || t.oid::pg_catalog.regtype::text AS described
    FROM pg_catalog.pg_type AS t
) AS held
INNER JOIN app AS ap ON held.owner_oid = ap.oid

UNION ALL
SELECT r.rolname || ' skips row-level security'
FROM (
    SELECT * FROM app
    UNION ALL
    SELECT * FROM bound_roles
) AS r
WHERE r.rolsuper OR r.rolbypassrls

UNION ALL
SELECT r.rolname || ' can create roles or databases, or replicate'
FROM (
    SELECT * FROM app
    UNION ALL
    SELECT * FROM bound_roles
) AS r
WHERE r.rolcreaterole OR r.rolcreatedb OR r.rolreplication

UNION ALL
SELECT br.rolname || ' can log in, but a module or lookup role never does'
FROM bound_roles AS br
WHERE br.rolcanlogin

-- A superuser owner is allowed, as on a local test server; a managed host's
-- role that skips row-level security without being one is not.
UNION ALL
SELECT 'the owner ' || r.rolname || ' skips row-level security'
FROM pg_catalog.pg_roles AS r
WHERE
    r.oid IN (SELECT o.oid FROM owners AS o)
    AND r.rolbypassrls
    AND NOT r.rolsuper

-- Nothing of Tetsu's lies outside the module schemas, where no rule reaches.
UNION ALL
SELECT outside.described || ' lies outside every module''s schema'
FROM (
    SELECT
        c.oid,
        c.relnamespace AS namespace,
        'pg_catalog.pg_class'::pg_catalog.regclass AS catalog,
        c.oid::pg_catalog.regclass::text AS described
    FROM pg_catalog.pg_class AS c
    WHERE c.relkind IN ('r', 'p', 'v', 'm', 'f', 'S')
    UNION ALL
    SELECT
        p.oid,
        p.pronamespace AS namespace,
        'pg_catalog.pg_proc'::pg_catalog.regclass AS catalog,
        'function ' || p.oid::pg_catalog.regprocedure::text AS described
    FROM pg_catalog.pg_proc AS p
) AS outside
INNER JOIN pg_catalog.pg_namespace AS n ON outside.namespace = n.oid
WHERE
    n.nspname NOT IN (SELECT m.schema_name FROM modules AS m)
    AND n.nspname NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
    AND n.nspname NOT LIKE 'pg\_temp\_%'
    AND n.nspname NOT LIKE 'pg\_toast\_temp\_%'
    AND NOT EXISTS (
        SELECT FROM pg_catalog.pg_depend AS d
        WHERE
            d.classid = outside.catalog
            AND d.objid = outside.oid
            AND d.deptype = 'e'
    )

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
    (
        s.setdatabase = 0 OR s.setdatabase IN (
            SELECT d.oid FROM pg_catalog.pg_database AS d
            WHERE d.datname = current_database()
        )
    )
    AND (
        s.setrole = 0
        OR s.setrole IN (SELECT ap.oid FROM app AS ap)
        OR s.setrole IN (SELECT o.oid FROM owners AS o)
        OR s.setrole IN (SELECT br.oid FROM bound_roles AS br)
    )
    AND (
        lower(split_part(setting.value, '=', 1))
        IN ('role', 'search_path', 'session_authorization')
        OR lower(split_part(setting.value, '=', 1)) LIKE 'app.%'
    )

UNION ALL
SELECT
    'function '
    || p.oid::pg_catalog.regprocedure::text
    || ' runs as its owner but is not owned by '
    || m.lookup_role
FROM pg_catalog.pg_proc AS p
INNER JOIN modules AS m ON p.pronamespace = m.oid
WHERE p.prosecdef AND p.proowner::pg_catalog.regrole::text <> m.lookup_role

UNION ALL
SELECT
    'function '
    || p.oid::pg_catalog.regprocedure::text
    || ' runs as its owner but sets no search_path of pg_catalog, pg_temp'
FROM pg_catalog.pg_proc AS p
INNER JOIN modules AS m ON p.pronamespace = m.oid
WHERE
    p.prosecdef
    AND NOT 'search_path=pg_catalog, pg_temp' = any(coalesce(p.proconfig, '{}'))

UNION ALL
SELECT 'function ' || g.shown || ' runs as its owner but anyone may run it'
FROM grants AS g
WHERE
    g.kind = 'function'
    AND g.definer
    AND g.grantee = 0
    AND g.privilege_type = 'EXECUTE'

ORDER BY 1;
