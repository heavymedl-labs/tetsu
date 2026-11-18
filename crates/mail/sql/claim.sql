-- Takes a key for a route, and says whether it did, and how the key stood
-- before: free, still being sent, or settled. A claim still sending after two
-- minutes is taken again.
WITH taken AS (
    INSERT INTO mail.sends AS s (message_key, route)
    VALUES ($1, $2)
    ON CONFLICT (message_key) DO UPDATE
        SET route = excluded.route, claimed_at = now()
        WHERE
        s.route = 'sending'
        AND s.claimed_at < now() - interval '2 minutes'
    RETURNING s.message_key
)

SELECT
    (SELECT count(*) FROM taken) = 1 AS taken,
    (
        SELECT sends.route FROM mail.sends AS sends
        WHERE sends.message_key = $1
    ) AS route;
