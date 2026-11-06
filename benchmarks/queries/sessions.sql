--! set_tenant
SELECT set_config('app.tenant_id', :tenant, true);

--! lookup
SELECT
    id,
    tenant_id,
    name,
    starts_at,
    capacity,
    booked,
    coach,
    room
FROM class_session
WHERE id = :id;

--! list
SELECT
    id,
    tenant_id,
    name,
    starts_at,
    capacity,
    booked,
    coach,
    room
FROM class_session
WHERE starts_at > :since
ORDER BY starts_at
LIMIT 50;

--! book_update
UPDATE class_session SET booked = booked + 1
WHERE id = :id;

--! book_insert
INSERT INTO booking (tenant_id, session_id, customer_id)
VALUES (:tenant_id, :session_id, :customer_id);
