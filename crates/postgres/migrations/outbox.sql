-- A module's outboxes (ADR-0043), included as one of its own migrations, so
-- it runs in the module's schema. The delivery role reads and settles every
-- tenant's rows, and nothing else.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE TABLE outbox_global (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    event_type text NOT NULL,
    event_version bigint NOT NULL,
    occurred_at timestamptz NOT NULL DEFAULT now(),
    payload jsonb NOT NULL,
    keep boolean NOT NULL DEFAULT FALSE,
    attempts bigint NOT NULL DEFAULT 0,
    due_at timestamptz NOT NULL DEFAULT now(),
    delivered_at timestamptz
);

COMMENT ON TABLE outbox_global IS
'tetsu:global: events that belong to no business, such as an operator''s';

-- The same columns, for one business's events.
CREATE TABLE outbox (
    tenant_id bigint NOT NULL,
    LIKE outbox_global INCLUDING DEFAULTS,
    PRIMARY KEY (tenant_id, id)
);

COMMENT ON TABLE outbox IS 'tetsu:tenant-owned';

ALTER TABLE outbox ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY;

-- The module's own role, named from the schema the runner put first on the
-- search path, writes events only for the tenant it acts for.
DO $$
DECLARE module_role text := pg_catalog.format('tetsu_%s', current_schema());
BEGIN
    EXECUTE pg_catalog.format(
        'CREATE POLICY tenant ON outbox TO %I USING (tenant_id = '
        '(SELECT nullif(current_setting(''app.tenant_id'', true), '''')::bigint))',
        module_role
    );
    EXECUTE pg_catalog.format(
        'CREATE POLICY tenant_only ON outbox AS RESTRICTIVE TO %I USING '
        '(tenant_id = (SELECT nullif(current_setting(''app.tenant_id'', true), '''')::bigint))',
        module_role
    );
    EXECUTE pg_catalog.format(
        'REVOKE ALL ON outbox, outbox_global FROM %I', module_role
    );
    EXECUTE pg_catalog.format(
        'GRANT INSERT, SELECT ON outbox, outbox_global TO %I', module_role
    );
    EXECUTE pg_catalog.format(
        'GRANT USAGE ON SCHEMA %I TO tetsu_delivery', current_schema()
    );
END $$;

-- The delivery role works across tenants by design, on these tables alone.
CREATE POLICY delivery ON outbox TO tetsu_delivery USING (TRUE);

-- It settles events, so it may change only when each is due and delivered,
-- its attempts, and whether it is kept, never what an event says.
GRANT SELECT, DELETE ON outbox, outbox_global TO tetsu_delivery;
GRANT UPDATE (attempts, due_at, delivered_at, keep) ON outbox, outbox_global
TO tetsu_delivery;

-- What is due, oldest first, for the dispatcher's polls.
CREATE INDEX outbox_due ON outbox (due_at) WHERE delivered_at IS NULL;
CREATE INDEX outbox_global_due ON outbox_global (due_at)
WHERE delivered_at IS NULL;
