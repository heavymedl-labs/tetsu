-- Forgets kept messages after a week, and the record of a key after thirty
-- days, by when no consumer still retries it.
WITH forgotten AS (
    DELETE FROM mail.captured
    WHERE captured_at < now() - interval '7 days'
)

DELETE FROM mail.sends
WHERE claimed_at < now() - interval '30 days';
