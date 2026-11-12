-- A tenant-owned table as ADR-0042 shapes one, for the isolation tests.
CREATE TABLE people (
    tenant_id bigint NOT NULL,
    id uuid NOT NULL DEFAULT uuidv7(),
    email text NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, email)
);

COMMENT ON TABLE people IS 'tetsu:tenant-owned';

ALTER TABLE people ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant ON people TO tetsu_people
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint)
);

CREATE POLICY tenant_only ON people AS RESTRICTIVE TO tetsu_people
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', true), '')::bigint)
);
