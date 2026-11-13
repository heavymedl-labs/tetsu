-- A consuming module's inboxes (ADR-0043), included as one of its own
-- migrations: the IDs of the events it has applied, each recorded in the
-- transaction that applies it, so a repeat is skipped.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE TABLE inbox (
    tenant_id bigint NOT NULL,
    id uuid NOT NULL,
    applied_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);

COMMENT ON TABLE inbox IS 'tetsu:tenant-owned';

CREATE TABLE inbox_global (
    id uuid PRIMARY KEY,
    applied_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE inbox_global IS
'tetsu:global: events applied that belong to no business';

ALTER TABLE inbox ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY;

DO $$
DECLARE module_role text := pg_catalog.format('tetsu_%s', current_schema());
BEGIN
    EXECUTE pg_catalog.format(
        'CREATE POLICY tenant ON inbox TO %I USING (tenant_id = '
        '(SELECT nullif(current_setting(''app.tenant_id'', true), '''')::bigint))',
        module_role
    );
    EXECUTE pg_catalog.format(
        'CREATE POLICY tenant_only ON inbox AS RESTRICTIVE TO %I USING '
        '(tenant_id = (SELECT nullif(current_setting(''app.tenant_id'', true), '''')::bigint))',
        module_role
    );
    EXECUTE pg_catalog.format(
        'REVOKE ALL ON inbox, inbox_global FROM %I', module_role
    );
    EXECUTE pg_catalog.format(
        'GRANT INSERT, SELECT ON inbox, inbox_global TO %I', module_role
    );
END $$;
