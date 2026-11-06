-- Benchmark schema: classes and bookings for 200 tenants, isolated by
-- row-level security for the non-superuser `app` role.
CREATE TABLE class_session (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id bigint NOT NULL,
    name text NOT NULL,
    starts_at timestamptz NOT NULL,
    capacity int NOT NULL,
    booked int NOT NULL DEFAULT 0,
    coach text NOT NULL,
    room text NOT NULL
);
CREATE INDEX ON class_session (tenant_id, starts_at);
CREATE TABLE booking (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id bigint NOT NULL,
    session_id bigint NOT NULL,
    customer_id bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE class_session ENABLE ROW LEVEL SECURITY;
ALTER TABLE booking ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant ON class_session
USING (tenant_id = current_setting('app.tenant_id')::bigint);
CREATE POLICY tenant ON booking
USING (tenant_id = current_setting('app.tenant_id')::bigint)
WITH CHECK (tenant_id = current_setting('app.tenant_id')::bigint);
GRANT SELECT, INSERT, UPDATE ON class_session, booking TO app;
-- Session ids for tenant t satisfy (id % 200) + 1 = t.
INSERT INTO class_session (tenant_id, name, starts_at, capacity, coach, room)
SELECT
    (g % 200) + 1 AS tenant_id,
    'Strength ' || g AS name,
    now() + (g % 2000) * interval '15 minutes' AS starts_at,
    20 AS capacity,
    'Coach ' || (g % 30) AS coach,
    'Room ' || (g % 4) AS room
FROM generate_series(1, 400000) AS g;
ANALYZE;
