-- Tenants, the addresses that reach them, and their locations. The database
-- bounds what Tenancy's domain bounds (ADR-0060).
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- A slug is 1 to 63 lowercase letters, digits, and inner hyphens, and a color
-- is #RRGGBB in capitals, as the domain writes it.
CREATE TABLE tenancy.tenants (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    slug text NOT NULL UNIQUE CONSTRAINT tenants_slug CHECK (
        octet_length(slug) BETWEEN 1 AND 63
        AND slug ~ '^[a-z0-9]+(-[a-z0-9]+)*$'
    ),
    name text NOT NULL CONSTRAINT tenants_name CHECK (
        char_length(name) BETWEEN 1 AND 100
    ),
    primary_color text NOT NULL CONSTRAINT tenants_primary_color CHECK (
        octet_length(primary_color) = 7 AND primary_color ~ '^#[0-9A-F]{6}$'
    ),
    accent_color text NOT NULL CONSTRAINT tenants_accent_color CHECK (
        octet_length(accent_color) = 7 AND accent_color ~ '^#[0-9A-F]{6}$'
    ),
    created_at timestamptz NOT NULL DEFAULT now()
);

-- Read before any tenant is known, so a tenant setting cannot guard it (#8).
-- An address is a path today, and a host name once a business brings its own
-- domain, which DNS limits to 253 characters.
CREATE TABLE tenancy.addresses (
    address text PRIMARY KEY CONSTRAINT addresses_address CHECK (
        octet_length(address) BETWEEN 2 AND 253
    ),
    tenant_id bigint NOT NULL REFERENCES tenancy.tenants (id)
);

CREATE INDEX addresses_tenant_id ON tenancy.addresses (tenant_id);

-- Each location has its own id, so nothing that refers to one depends on its
-- place in the list. A place stays unique, checked at commit, so a reorder can
-- swap two in one transaction.
CREATE TABLE tenancy.locations (
    tenant_id bigint NOT NULL REFERENCES tenancy.tenants (id),
    id uuid NOT NULL DEFAULT uuidv7(),
    sort_order bigint NOT NULL,
    name text NOT NULL CONSTRAINT locations_name CHECK (
        char_length(name) BETWEEN 1 AND 100
    ),
    time_zone text NOT NULL CONSTRAINT locations_time_zone CHECK (
        char_length(time_zone) BETWEEN 1 AND 100
    ),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT locations_place UNIQUE (tenant_id, sort_order)
    DEFERRABLE INITIALLY DEFERRED
);
