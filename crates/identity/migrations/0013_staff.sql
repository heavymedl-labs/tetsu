-- Staff, who work for businesses, kept apart from admins (ADR-0088,
-- ADR-0089, ADR-0090): a staff member is one identity across the businesses
-- they work for, with their own passkeys, ceremonies, and sessions, all
-- global as an admin's are, and a role and invites at each business, which
-- belong to that business (ADR-0042). Functions that use them come with the
-- staff store.
SET lock_timeout = '5s';
SET statement_timeout = '30s';

CREATE TABLE staff (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- Not unique: two people may share a name. No control character or line
    -- break, so no name can start a new line of output.
    name text NOT NULL CHECK (
        char_length(name) BETWEEN 1 AND 100
        AND name !~ '[[:cntrl:]  ]'
    ),
    -- A plain address, as tetsu_kernel::Address writes it, and only the staff
    -- member changes it (ADR-0040).
    email text NOT NULL CHECK (
        octet_length(email) BETWEEN 3 AND 254
        AND email !~ '[[:cntrl:][:space:]"<>]'
    ),
    -- WebAuthn's user handle: random, so it says nothing about the person.
    user_handle uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    created_at timestamptz NOT NULL DEFAULT now(),
    -- Disabled rather than removed, so the audit record keeps a name to point
    -- at; a disabled staff member starts and uses no session.
    disabled_at timestamptz
);

COMMENT ON TABLE staff IS
'tetsu:global: a staff member is one identity across businesses (ADR-0040)';

-- An email belongs to one staff member, whatever its case (ADR-0040).
CREATE UNIQUE INDEX staff_email ON staff (lower(email));

-- A session is kept under its token's SHA-256 hash, never the token
-- (ADR-0047), and lasts as ADR-0048 sets for staff.
CREATE TABLE staff_sessions (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    -- What a list of a person's sessions shows and ends them by.
    id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    realm text NOT NULL CHECK (realm IN ('staff')),
    staff_id bigint NOT NULL REFERENCES staff (id),
    started_at timestamptz NOT NULL,
    -- When a passkey last proved the person in this session, as for admins.
    proven_at timestamptz,
    last_seen_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL,
    -- The device a session started on, as coarsely as a list shows it.
    device text CHECK (device IS NULL OR length(device) BETWEEN 1 AND 64)
)
-- Every logged-in request rewrites last_seen_at, which no index names, so
-- room on the page keeps each rewrite there (ADR-0060).
WITH (fillfactor = 80);

COMMENT ON TABLE staff_sessions IS
'tetsu:global: found by its token before any business is known';

CREATE INDEX staff_sessions_staff_id ON staff_sessions (staff_id);

-- A staff member's passkeys, as webauthn-rs keeps each: a public key, a
-- signature counter, and flags, never a secret.
CREATE TABLE staff_passkeys (
    credential_id bytea PRIMARY KEY CHECK (
        octet_length(credential_id) >= 16
        AND octet_length(credential_id) <= 1023
    ),
    staff_id bigint NOT NULL REFERENCES staff (id),
    passkey jsonb NOT NULL CHECK (octet_length(passkey::text) <= 8320),
    -- How the browser said it can reach the passkey, as for admins.
    transports text NOT NULL DEFAULT '' CHECK (
        char_length(transports) <= 27
        AND transports
        ~ '^((ble|hybrid|internal|nfc|usb)( (ble|hybrid|internal|nfc|usb))*)?$'
    ),
    created_at timestamptz NOT NULL,
    last_used_at timestamptz
);

COMMENT ON TABLE staff_passkeys IS
'tetsu:global: found by its credential before anyone is logged in';

CREATE INDEX staff_passkeys_staff_id ON staff_passkeys (staff_id);

-- A passkey ceremony under way, as for admins: its state under the hash of a
-- token only the browser holds, good once and for ten minutes. A join keeps
-- the invite link it will redeem.
CREATE TABLE staff_ceremonies (
    token_hash bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    kind text NOT NULL CHECK (kind IN ('join', 'login', 'add')),
    staff_id bigint REFERENCES staff (id),
    link_hash bytea CHECK (octet_length(link_hash) = 32),
    session_hash bytea CHECK (octet_length(session_hash) = 32)
    REFERENCES staff_sessions (token_hash) ON DELETE CASCADE,
    taken boolean NOT NULL DEFAULT FALSE,
    state jsonb NOT NULL CHECK (octet_length(state::text) <= 16384),
    ends_at timestamptz NOT NULL,
    -- A join and an addition know their staff member; only a join keeps a
    -- link, and only an addition a session, which alone is taken.
    CONSTRAINT staff_ceremonies_whose CHECK (
        (kind IN ('join', 'add')) = (staff_id IS NOT NULL)
    ),
    CONSTRAINT staff_ceremonies_link CHECK (
        (kind = 'join') = (link_hash IS NOT NULL)
    ),
    CONSTRAINT staff_ceremonies_session CHECK (
        (kind = 'add') = (session_hash IS NOT NULL)
    ),
    CONSTRAINT staff_ceremonies_taken CHECK (kind = 'add' OR NOT taken)
);

COMMENT ON TABLE staff_ceremonies IS
'tetsu:global: found by its token before anyone is logged in';

CREATE INDEX staff_ceremonies_staff_id ON staff_ceremonies (staff_id);
CREATE UNIQUE INDEX staff_ceremonies_link_hash ON staff_ceremonies (link_hash);
CREATE INDEX staff_ceremonies_ends_at ON staff_ceremonies (ends_at);
CREATE UNIQUE INDEX staff_ceremonies_addition ON staff_ceremonies (staff_id)
WHERE kind = 'add';
CREATE INDEX staff_ceremonies_session_hash ON staff_ceremonies (session_hash)
WHERE session_hash IS NOT NULL;

-- A staff member's role at one business (ADR-0048): one each, owned by the
-- business, under its row security.
CREATE TABLE staff_roles (
    tenant_id bigint NOT NULL,
    id uuid NOT NULL DEFAULT uuidv7(),
    staff_id bigint NOT NULL REFERENCES staff (id),
    role_name text NOT NULL CHECK (
        role_name IN ('owner', 'manager', 'front_desk', 'instructor')
    ),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, staff_id)
);

COMMENT ON TABLE staff_roles IS 'tetsu:tenant-owned';

CREATE INDEX staff_roles_staff_id ON staff_roles (staff_id);

-- An open invite to a business, under its link's SHA-256 hash, never the
-- link, good for seven days and once (ADR-0089). It names the staff member it
-- invites, made when invited if new, and the role joining gives.
CREATE TABLE staff_invites (
    tenant_id bigint NOT NULL,
    id uuid NOT NULL DEFAULT uuidv7(),
    token_hash bytea NOT NULL CHECK (octet_length(token_hash) = 32),
    staff_id bigint NOT NULL REFERENCES staff (id),
    role_name text NOT NULL CHECK (
        role_name IN ('owner', 'manager', 'front_desk', 'instructor')
    ),
    issued_at timestamptz NOT NULL,
    ends_at timestamptz NOT NULL,
    PRIMARY KEY (tenant_id, id),
    -- One open invite per person at each business; a new one replaces it.
    UNIQUE (tenant_id, staff_id)
);

COMMENT ON TABLE staff_invites IS 'tetsu:tenant-owned';

-- A link is found by its hash before its business is known.
CREATE INDEX staff_invites_token_hash ON staff_invites (token_hash);
CREATE INDEX staff_invites_staff_id ON staff_invites (staff_id);

ALTER TABLE staff_roles ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY;
ALTER TABLE staff_invites ENABLE ROW LEVEL SECURITY, FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant ON staff_roles TO tetsu_identity
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', TRUE), '')::bigint)
);

CREATE POLICY tenant_only ON staff_roles AS RESTRICTIVE TO tetsu_identity
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', TRUE), '')::bigint)
);

CREATE POLICY tenant ON staff_invites TO tetsu_identity
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', TRUE), '')::bigint)
);

CREATE POLICY tenant_only ON staff_invites AS RESTRICTIVE TO tetsu_identity
USING (
    tenant_id
    = (SELECT nullif(current_setting('app.tenant_id', TRUE), '')::bigint)
);

-- The invite lookup reads one invite by its link's hash, before its business
-- is known (ADR-0042, as clarified for ADR-0089).
CREATE POLICY lookup ON staff_invites FOR SELECT TO tetsu_identity_lookup
USING (TRUE);

COMMENT ON POLICY lookup ON staff_invites IS
'tetsu:lookup: an invite is found by its link before its business is known';

-- The runner's default privileges give Identity's role every right on a new
-- table; each is replaced by exactly what is granted below, and the staff
-- store's functions add theirs.
REVOKE ALL ON staff, staff_sessions, staff_passkeys, staff_ceremonies,
staff_roles, staff_invites
FROM tetsu_identity;
REVOKE ALL ON SEQUENCE staff_id_seq FROM tetsu_identity;

-- Identity's role reads and writes a business's roles and invites in that
-- business's own transaction, under its row security.
GRANT SELECT, INSERT, DELETE ON staff_roles, staff_invites TO tetsu_identity;
