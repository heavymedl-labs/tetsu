-- The audit record (ADR-0045): one row per event, kept per tenant, or
-- globally for events that belong to no business, such as an operator's
-- sign-in. Rows are only ever added.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE TABLE records_global (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    event_id uuid NOT NULL UNIQUE,
    producer text NOT NULL,
    event_type text NOT NULL,
    event_version bigint NOT NULL,
    occurred_at timestamptz NOT NULL,
    payload jsonb NOT NULL,
    recorded_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE records_global IS
'tetsu:global: security events that belong to no business';

-- The same columns, for one business's events.
CREATE TABLE records (
    tenant_id bigint NOT NULL,
    LIKE records_global INCLUDING DEFAULTS,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, event_id)
);

COMMENT ON TABLE records IS 'tetsu:tenant-owned';

ALTER TABLE records ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant ON records TO tetsu_audit
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint)
);

CREATE POLICY tenant_only ON records AS RESTRICTIVE TO tetsu_audit
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint)
);

-- The record is append-only: its role adds and reads rows, and never changes
-- or removes one.
REVOKE UPDATE, DELETE ON records, records_global FROM tetsu_audit;
