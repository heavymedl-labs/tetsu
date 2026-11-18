DELETE FROM mail.captured
WHERE captured_at < now() - interval '7 days';
