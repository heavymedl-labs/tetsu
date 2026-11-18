INSERT INTO mail.sends (message_key, route)
VALUES ($1, $2)
ON CONFLICT (message_key) DO NOTHING
RETURNING message_key;
