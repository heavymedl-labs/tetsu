-- The audit record (ADR-0045): one row per event, kept per tenant, or
-- globally for events that belong to no business, such as an operator's
-- login. Rows are only ever added.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- A producer is a module's schema, an event's type is `module.fact`, and a
-- payload is a JSON object of IDs and facts, bounded as the outbox bounds them
-- (ADR-0060).
CREATE TABLE records_global (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    event_id uuid NOT NULL UNIQUE,
    producer text NOT NULL CHECK (octet_length(producer) BETWEEN 1 AND 63),
    event_type text NOT NULL CHECK (
        octet_length(event_type) <= 100
        AND event_type ~ '^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$'
    ),
    event_version bigint NOT NULL,
    occurred_at timestamptz NOT NULL,
    payload jsonb NOT NULL CHECK (
        jsonb_typeof(payload) = 'object'
        AND octet_length(payload::text) <= 65536
    ),
    recorded_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE records_global IS
'tetsu:global: security events that belong to no business';

-- The same columns, for one business's events.
CREATE TABLE records (
    tenant_id bigint NOT NULL,
    LIKE records_global INCLUDING DEFAULTS INCLUDING CONSTRAINTS,
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
-- or removes one, whatever the schema's defaults grant.
REVOKE ALL ON records, records_global FROM tetsu_audit;
GRANT SELECT, INSERT ON records, records_global TO tetsu_audit;
