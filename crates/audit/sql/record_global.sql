-- An event that belongs to no business.
INSERT INTO audit.records_global (
    event_id, producer, event_type, event_version, occurred_at, payload
)
VALUES ($1::text::uuid, $2, $3, $4, $5, $6::text::jsonb)
