DELETE FROM mail.sends
WHERE message_key = $1 AND route = 'sent';
