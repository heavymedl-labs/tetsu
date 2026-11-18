SELECT
    recipient,
    subject,
    text_body,
    html_body,
    captured_at
FROM mail.captured
WHERE captured_at >= now() - interval '7 days'
ORDER BY captured_at DESC, message_key ASC
LIMIT 100;
