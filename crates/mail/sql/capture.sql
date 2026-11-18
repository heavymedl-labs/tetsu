INSERT INTO mail.captured (
    message_key, recipient, subject, text_body, html_body
)
VALUES ($1, $2, $3, $4, $5);
