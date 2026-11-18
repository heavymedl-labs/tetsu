-- Settles a key this attempt still holds as sent or refused, or, with no
-- route, gives it back, so the next attempt tries again at once.
WITH released AS (
    DELETE FROM mail.sends
    WHERE message_key = $1 AND route = 'sending' AND $2::text IS NULL
)

UPDATE mail.sends
SET route = $2
WHERE message_key = $1 AND route = 'sending' AND $2::text IS NOT NULL;
