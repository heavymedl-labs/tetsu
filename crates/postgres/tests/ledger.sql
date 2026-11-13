-- What the test consumer does with each event: one row per event applied,
-- for a tenant or for no business.
CREATE TABLE effects (
    tenant_id bigint NOT NULL,
    id uuid NOT NULL DEFAULT uuidv7(),
    event_id uuid NOT NULL,
    PRIMARY KEY (tenant_id, id)
);

COMMENT ON TABLE effects IS 'tetsu:tenant-owned';

ALTER TABLE effects ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant ON effects TO tetsu_ledger
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint)
);

CREATE POLICY tenant_only ON effects AS RESTRICTIVE TO tetsu_ledger
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint)
);

CREATE TABLE global_effects (
    id uuid PRIMARY KEY DEFAULT uuidv7(),
    event_id uuid NOT NULL
);

COMMENT ON TABLE global_effects IS 'tetsu:global: test';
