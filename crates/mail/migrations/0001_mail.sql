-- Mail (ADR-0044, ADR-0074): each message handed to the Mailer, once per
-- key, and the messages kept for the mailbox page rather than sent. Both are
-- global: they hold mail to operators, who belong to no business. A
-- business's mail gets tables of its own when staff mail arrives (#111).
SET lock_timeout = '5s';
SET statement_timeout = '30s';

-- A key names the event that caused a message, so it is sent once however
-- often it is handed over. A row is claimed before a message goes out and
-- removed if sending fails, so a later attempt may try again. It holds no
-- address and no body.
CREATE TABLE sends (
    message_key text PRIMARY KEY CHECK (
        octet_length(message_key) BETWEEN 1 AND 128
        AND message_key ~ '^[A-Za-z0-9._:-]+$'
    ),
    route text NOT NULL CHECK (route IN ('captured', 'sent')),
    claimed_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE sends IS
'tetsu:global: mail to operators belongs to no business';

-- A message kept rather than sent, as the mailbox page shows it. Kept a
-- week, since it may hold a link, and the page is for reading what was just
-- sent.
CREATE TABLE captured (
    message_key text PRIMARY KEY
    REFERENCES sends (message_key) ON DELETE CASCADE,
    recipient text NOT NULL CHECK (
        octet_length(recipient) BETWEEN 3 AND 254
        AND recipient !~ '[[:cntrl:][:space:]]'
    ),
    subject text NOT NULL CHECK (
        char_length(subject) <= 998
        AND subject !~ '[[:cntrl:]]'
    ),
    text_body text NOT NULL CHECK (octet_length(text_body) <= 1048576),
    html_body text NOT NULL CHECK (octet_length(html_body) <= 1048576),
    captured_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE captured IS
'tetsu:global: mail to operators belongs to no business';

CREATE INDEX captured_newest ON captured (captured_at DESC);
