-- One business's event, under the tenant the event names, which the tenant
-- policy checks against the transaction's own.
INSERT INTO audit.records (
    tenant_id,
    event_id,
    producer,
    event_type,
    event_version,
    occurred_at,
    payload
)
VALUES ($7, $1::text::uuid, $2, $3, $4, $5, $6::text::jsonb)
