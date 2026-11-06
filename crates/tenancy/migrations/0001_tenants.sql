-- Tenants, the addresses that reach them, and their locations.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE TABLE tenancy.tenants (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    slug text NOT NULL UNIQUE,
    name text NOT NULL,
    primary_color text NOT NULL,
    accent_color text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- Read before any tenant is known, so a tenant setting cannot guard it (#8).
CREATE TABLE tenancy.addresses (
    address text PRIMARY KEY,
    tenant_id bigint NOT NULL REFERENCES tenancy.tenants (id)
);

CREATE INDEX addresses_tenant_id ON tenancy.addresses (tenant_id);

CREATE TABLE tenancy.locations (
    tenant_id bigint NOT NULL REFERENCES tenancy.tenants (id),
    sort_order bigint NOT NULL,
    name text NOT NULL,
    time_zone text NOT NULL,
    PRIMARY KEY (tenant_id, sort_order)
);
