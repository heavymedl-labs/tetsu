SELECT
    recipient,
    subject,
    text_body,
    html_body,
    captured_at
FROM mail.captured
ORDER BY captured_at DESC, message_key ASC
LIMIT 100;
